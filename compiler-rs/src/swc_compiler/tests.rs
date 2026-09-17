use super::SwcCompiler;

#[test]
fn test_swc_typescript_stripping() {
    let ts_code = r#"
interface User {
    id: number;
    name: string;
    email?: string;
}

type Role = "admin" | "user" | "guest";

function formatUser<T extends User>(user: T, role: Role): string {
    const isOwner: boolean = role === "admin";
    return `${user.name} (${role}) - owner: ${isOwner}`;
}

const config = {
    timeout: 5000,
    retries: 3
} as const;

export { formatUser, config };
"#;

    let result = SwcCompiler::compile(ts_code, "test.ts").expect("Failed to compile TypeScript");
    
    // Type definitions must be stripped
    assert!(!result.contains("interface User"));
    assert!(!result.contains("type Role"));
    assert!(!result.contains("<T extends User>"));
    assert!(!result.contains(": boolean"));
    assert!(!result.contains("as const"));
    
    // Executable code must be preserved
    assert!(result.contains("formatUser"));
    assert!(result.contains("config"));
    assert!(result.contains("timeout: 5000"));
}

#[test]
fn test_swc_react_jsx_transform() {
    let jsx_code = r#"
import React from 'react';

export default function SimpleButton({ label, onClick }) {
    return (
        <button className="px-4 py-2 bg-blue-500 text-white rounded" onClick={onClick}>
            <span>{label}</span>
        </button>
    );
}
"#;

    let result = SwcCompiler::compile(jsx_code, "Button.jsx").expect("Failed to compile JSX");

    // Must be transformed to React.createElement
    assert!(result.contains("React.createElement"));
    assert!(result.contains("\"button\""));
    assert!(result.contains("\"span\""));
    assert!(result.contains("className: \"px-4 py-2 bg-blue-500 text-white rounded\""));
    assert!(result.contains("export default function SimpleButton"));
}

#[test]
fn test_swc_react_fragments_and_nesting() {
    let tsx_code = r#"
import React from 'react';

export const FragmentComponent = () => {
    return (
        <>
            <h1>Title</h1>
            <p>Description</p>
        </>
    );
};
"#;

    let result = SwcCompiler::compile(tsx_code, "Fragment.tsx").expect("Failed to compile TSX Fragment");

    assert!(result.contains("React.createElement"));
    assert!(result.contains("React.Fragment") || result.contains("FragmentComponent"));
    assert!(result.contains("\"h1\""));
    assert!(result.contains("\"p\""));
}

#[test]
fn test_swc_full_tsx_component_with_hooks() {
    let tsx_code = r#"
import React, { useState, useEffect } from 'react';

interface CounterProps {
    initialCount?: number;
    step?: number;
}

export default function Counter({ initialCount = 0, step = 1 }: CounterProps) {
    const [count, setCount] = useState<number>(initialCount);

    useEffect(() => {
        console.log("Count changed:", count);
    }, [count]);

    const handleIncrement = () => {
        setCount(prev => prev + step);
    };

    return (
        <div className="counter-container">
            <h2 className="text-xl font-bold">Count: {count}</h2>
            <button onClick={handleIncrement} className="btn-primary">
                Increment +{step}
            </button>
        </div>
    );
}
"#;

    let result = SwcCompiler::compile(tsx_code, "Counter.tsx").expect("Failed to compile TSX with hooks");

    // TypeScript types stripped
    assert!(!result.contains("interface CounterProps"));
    assert!(!result.contains("<number>"));
    assert!(!result.contains(": CounterProps"));

    // React JSX compiled to React.createElement
    assert!(result.contains("React.createElement"));
    assert!(result.contains("\"div\""));
    assert!(result.contains("\"h2\""));
    assert!(result.contains("\"button\""));

    // Logic preserved
    assert!(result.contains("useState"));
    assert!(result.contains("useEffect"));
    assert!(result.contains("handleIncrement"));
}

#[test]
fn test_swc_preserves_esm_imports_and_exports() {
    let tsx_code = r#"
import React from "https://esm.sh/react@18.3.1?dev";
import { Header } from "/_bundle/components/Header";
import { formatCurrency } from "/_bundle/lib/utils";

export const metadata = {
    title: "Checkout Page",
    description: "Secure payment gateway"
};

export default function CheckoutPage() {
    return (
        <div className="checkout">
            <Header title="Payment" />
            <span>{formatCurrency(100000)}</span>
        </div>
    );
}
"#;

    let result = SwcCompiler::compile(tsx_code, "app/checkout/page.tsx").expect("Failed to compile Page");

    // Preserves ESM import and export statements
    assert!(result.contains("import { Header } from \"/_bundle/components/Header\";"));
    assert!(result.contains("import { formatCurrency } from \"/_bundle/lib/utils\";"));
    assert!(result.contains("export const metadata ="));
    assert!(result.contains("export default function CheckoutPage"));
    assert!(result.contains("React.createElement"));
}

#[test]
fn test_swc_handles_syntax_errors_gracefully() {
    let broken_code = r#"
export default function Broken() {
    return <div unclosed_tag>;
}
"#;

    let result = SwcCompiler::compile(broken_code, "broken.tsx");
    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(err_msg.contains("SWC Parse Error") || err_msg.contains("broken.tsx"));
}

#[test]
fn test_swc_server_action_proxy_compilation() {
    let proxy_code = r#"
export const processPayment = new Proxy({}, {
    get: (target, prop) => {
        return async (...args) => {
            const res = await fetch("/_nata/rpc", {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                    module: "modules/payment",
                    action: String(prop),
                    args: args
                })
            });
            if (!res.ok) throw new Error("RPC failed");
            return await res.json();
        };
    }
});
"#;

    let result = SwcCompiler::compile(proxy_code, "actions.ts").expect("Failed to compile server action proxy");
    assert!(result.contains("export const processPayment = new Proxy"));
    assert!(result.contains("/_nata/rpc"));
}
