use maud::{html, Markup};

pub fn card(active: bool) -> Markup {
    html! {
        div.flex.items-center.gap-4 class=(if active { "bg-blue-500" } else { "bg-gray-100" }) {
            p.text-black[active] { "conditional" }
            span class="px-2 py-1 rounded" { "x" }
        }
    }
}

#[component]
fn Button(cx: Scope) -> impl IntoView {
    view! { cx, <button class:text-white=move || active() class="btn h-9">"Ok"</button> }
}
