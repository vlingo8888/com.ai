/**
 * Full in-browser Mock SDK Bridge for Zalo Mini App APIs.
 * Eliminates need for `zmp-sdk` during local dev/testing.
 */
export function getZmpMockBridgeScript(appId: string = "2840505191283749120"): string {
  return `
(function() {
  if (typeof window === 'undefined') return;

  const MOCK_USER = {
    id: "123456789012345678",
    idByOA: "oa_user_123456",
    name: "Zalo Tester",
    avatar: "https://h5.zdn.vn/static/images/avatar.png",
    gender: "male",
  };

  const MOCK_STORAGE = new Map();

  const ZmpNativeBridge = {
    // 1. User & Identity
    getUserInfo: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] getUserInfo()", "color: #0068ff; font-weight: bold;", MOCK_USER);
      options.success?.({ userInfo: MOCK_USER });
      return { userInfo: MOCK_USER };
    },
    getPhoneNumber: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] getPhoneNumber()", "color: #0068ff; font-weight: bold;", "0912345678");
      const res = { number: "0912345678", token: "mock_phone_token_xyz" };
      options.success?.(res);
      return res;
    },
    getSetting: async (options = {}) => {
      const res = {
        authSetting: {
          "scope.userInfo": true,
          "scope.userLocation": true,
          "scope.userPhonenumber": true,
        },
      };
      options.success?.(res);
      return res;
    },
    authorize: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] authorize()", "color: #0068ff; font-weight: bold;", options.scopes);
      options.success?.({ success: true });
      return { success: true };
    },
    login: async (options = {}) => {
      const res = { code: "mock_zalo_auth_code_12345" };
      options.success?.(res);
      return res;
    },

    // 2. Device & System
    getLocation: async (options = {}) => {
      const res = { latitude: 10.7769, longitude: 106.7009, provider: "gps" };
      options.success?.(res);
      return res;
    },
    getNetworkType: async (options = {}) => {
      const res = { networkType: "wifi" };
      options.success?.(res);
      return res;
    },
    getSystemInfo: async (options = {}) => {
      const res = {
        platform: "browser_simulator",
        system: "Simulator / Zalo Mini App Dev",
        version: "1.0.0",
        statusBarHeight: 44,
        windowWidth: 390,
        windowHeight: 844,
        pixelRatio: 3,
        appId: "${appId}",
      };
      options.success?.(res);
      return res;
    },

    // 3. Storage
    setStorage: async ({ key, data, success }) => {
      MOCK_STORAGE.set(key, data);
      success?.({ success: true });
      return { success: true };
    },
    getStorage: async ({ key, success }) => {
      const data = MOCK_STORAGE.get(key);
      const res = { data: data !== undefined ? data : null };
      success?.(res);
      return res;
    },
    removeStorage: async ({ key, success }) => {
      MOCK_STORAGE.delete(key);
      success?.({ success: true });
      return { success: true };
    },
    clearStorage: async ({ success } = {}) => {
      MOCK_STORAGE.clear();
      success?.({ success: true });
      return { success: true };
    },

    // 4. Interaction & UI
    openChat: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] openChat()", "color: #0068ff; font-weight: bold;", options);
      alert("[ZMP Mock] Mở cửa sổ chat Zalo: " + JSON.stringify(options));
      options.success?.({ success: true });
      return { success: true };
    },
    followOA: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] followOA()", "color: #0068ff; font-weight: bold;", options);
      alert("[ZMP Mock] Đã quan tâm Official Account: " + (options.id || "OA"));
      options.success?.({ success: true });
      return { success: true };
    },
    showToast: async (options = {}) => {
      const msg = typeof options === "string" ? options : options.message;
      console.log("%c[ZMP SDK Mock] showToast()", "color: #0068ff; font-weight: bold;", msg);
      return { success: true };
    },
    showLoading: async () => ({ success: true }),
    hideLoading: async () => ({ success: true }),
    createOrder: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] createOrder()", "color: #0068ff; font-weight: bold;", options);
      const res = { orderId: "zmp_order_" + Date.now(), success: true };
      options.success?.(res);
      return res;
    },
    closeApp: async () => {
      console.log("%c[ZMP SDK Mock] closeApp()", "color: #0068ff; font-weight: bold;");
      return { success: true };
    },
  };

  // Expose as window.ZMP, window.zmp, and native bridge
  window.ZMP = window.ZMP || ZmpNativeBridge;
  window.zmp = window.zmp || ZmpNativeBridge;
  window.__ZMP_SIMULATOR__ = true;
  window.__ZMP_APP_ID__ = "${appId}";
})();
`;
}
