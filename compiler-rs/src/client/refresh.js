// React Fast Refresh runtime setup, served at /_nata/refresh.js. Imported first by the router so it
// hooks into React before react-dom loads; compiled browser modules register their components with
// it (see `with_refresh_registration` in swc_compiler/module.rs).

// Relative to this module (Zalo documents are not served by the dev server)
import RefreshRuntime from "./deps/react-refresh/runtime";

RefreshRuntime.injectIntoGlobalHook(window);
self.$RefreshReg$ = () => {};
self.$RefreshSig$ = () => (type) => type;
self.__NATA_REFRESH__ = RefreshRuntime;
