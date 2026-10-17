"use client";

// Client components of the React Server Components router (a "use client" module). The same file is
// served to the browser (`/_nata/rsc/boundaries.js`) and imported by the SSR renderer
// (`.nata/rsc_boundaries.js`), so server-rendered markup hydrates against identical components.
//
// A page is a list of segment nodes, root to leaf: one per route folder (its layout, wrapping its
// template / error / loading / not-found boundaries around an <Outlet>), then the page. <Outlet depth>
// renders the node at that depth from SegmentsContext, which lets the client router keep the nodes of
// layouts it already has (same element, so React keeps their state) and swap only what changed.
//
// Boundaries inside a reused layout must still react to the segment below them changing (show
// loading.tsx right away, reset error.tsx, remount template.tsx): they take the depth of that
// segment and read its current key from KeysContext instead of a key baked into the element.

import { Component, createContext, createElement, Fragment, Suspense, useContext, useEffect } from "react";

export const SegmentsContext = createContext([]);
export const KeysContext = createContext([]);
export const SelectedContext = createContext([]);
export const RouterContext = createContext(null);

const useSegmentKey = (depth) => useContext(KeysContext)[depth] ?? "";

export function Outlet({ depth }) {
  const nodes = useContext(SegmentsContext);
  const selected = useContext(SelectedContext);
  return createElement(SelectedLayoutContext.Provider, { value: selected[depth] ?? [] }, nodes[depth] ?? null);
}

/** The URL segments below the layout being rendered (useSelectedLayoutSegment(s)) */
export const SelectedLayoutContext = createContext([]);

const digestOf = (error) => (error && typeof error === "object" && typeof error.digest === "string" ? error.digest : "");
export const isNotFoundError = (error) => digestOf(error) === "NEXT_NOT_FOUND";
export const isRedirectError = (error) => digestOf(error).startsWith("NEXT_REDIRECT");
export const redirectTarget = (error) => {
  const parts = digestOf(error).split(";");
  // NEXT_REDIRECT;<url> (this framework) or NEXT_REDIRECT;<type>;<url>;<status> (Next.js)
  return parts.length >= 4 ? parts[2] : parts.slice(1).join(";");
};

/** error.tsx: renders the segment's error component for errors thrown below it */
export function ErrorBoundary({ depth, ...props }) {
  return createElement(ErrorBoundaryImpl, { ...props, resetKey: useSegmentKey(depth) });
}

class ErrorBoundaryImpl extends Component {
  constructor(props) {
    super(props);
    this.state = { error: null, resetKey: props.resetKey };
    this.reset = () => this.setState({ error: null });
  }
  static getDerivedStateFromError(error) {
    // notFound() and redirect() are control flow, handled by their own boundaries
    if (isNotFoundError(error) || isRedirectError(error)) throw error;
    return { error };
  }
  static getDerivedStateFromProps(props, state) {
    // Navigating away resets the boundary, like Next.js
    if (props.resetKey !== state.resetKey) return { error: null, resetKey: props.resetKey };
    return null;
  }
  render() {
    if (this.state.error && this.props.errorComponent) {
      return createElement(this.props.errorComponent, { error: this.state.error, reset: this.reset });
    }
    return this.props.children;
  }
}

/** not-found.tsx: shown when notFound() is called below it */
export function NotFoundBoundary({ depth, ...props }) {
  return createElement(NotFoundBoundaryImpl, { ...props, resetKey: useSegmentKey(depth) });
}

class NotFoundBoundaryImpl extends Component {
  constructor(props) {
    super(props);
    this.state = { notFound: false, resetKey: props.resetKey };
  }
  static getDerivedStateFromError(error) {
    if (isNotFoundError(error)) return { notFound: true };
    throw error;
  }
  static getDerivedStateFromProps(props, state) {
    if (props.resetKey !== state.resetKey) return { notFound: false, resetKey: props.resetKey };
    return null;
  }
  render() {
    return this.state.notFound ? this.props.notFound ?? null : this.props.children;
  }
}

function Redirect({ to }) {
  const router = useContext(RouterContext);
  useEffect(() => {
    if (router) router.replace(to);
    else window.location.replace(to);
  }, [to]);
  return null;
}

/** redirect() during client navigation or after streaming started */
export class RedirectBoundary extends Component {
  constructor(props) {
    super(props);
    this.state = { to: null };
  }
  static getDerivedStateFromError(error) {
    if (isRedirectError(error)) return { to: redirectTarget(error) };
    throw error;
  }
  componentDidUpdate(prev) {
    if (this.state.to && prev.children !== this.props.children) this.setState({ to: null });
  }
  render() {
    return this.state.to ? createElement(Redirect, { to: this.state.to }) : this.props.children;
  }
}

/** loading.tsx: a Suspense boundary keyed by the child segment, so navigating shows it right away */
export function LoadingBoundary({ depth, fallback, children }) {
  return createElement(Suspense, { key: useSegmentKey(depth), fallback }, children);
}

/** template.tsx: remounted whenever the segment below it changes */
export function TemplateBoundary({ depth, children }) {
  return createElement(Fragment, { key: useSegmentKey(depth) }, children);
}

/** The whole page: providers around the root segment node (identical in SSR and in the browser) */
export function AppRoot({ nodes, keys, selected, router }) {
  const tree = createElement(SelectedLayoutContext.Provider, { value: (selected && selected[0]) || [] }, createElement(RedirectBoundary, null, nodes[0] ?? null));
  return createElement(
    RouterContext.Provider,
    { value: router },
    createElement(
      KeysContext.Provider,
      { value: keys || [] },
      createElement(SelectedContext.Provider, { value: selected || [] }, createElement(SegmentsContext.Provider, { value: nodes }, tree)),
    ),
  );
}
