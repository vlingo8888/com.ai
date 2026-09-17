//! Zalo Mini App Mock SDK Native Bridge for in-browser simulation

pub struct ZmpMockBridge;

impl ZmpMockBridge {
    /// Returns the client-side JavaScript snippet that initializes the ZMP Mock SDK
    pub fn get_browser_mock_script() -> &'static str {
        r#"
(function() {
  if (typeof window === 'undefined') return;

  const MOCK_USER = {
    id: "123456789012345678",
    idByOA: "oa_user_123456",
    name: "Zalo Test User",
    avatar: "https://h5.zdn.vn/static/images/avatar.png",
    gender: "male",
  };

  const MOCK_STORAGE = new Map();

  const ZmpNativeBridge = {
    // 1. User & Auth
    getUserInfo: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] getUserInfo()", "color: #0068ff; font-weight: bold;", MOCK_USER);
      return { userInfo: MOCK_USER };
    },
    getPhoneNumber: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] getPhoneNumber()", "color: #0068ff; font-weight: bold;", "0912345678");
      return { number: "0912345678", token: "mock_phone_token_xyz" };
    },
    getSetting: async (options = {}) => {
      return {
        authSetting: {
          "scope.userInfo": true,
          "scope.userLocation": true,
          "scope.userPhonenumber": true,
        },
      };
    },
    authorize: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] authorize()", "color: #0068ff; font-weight: bold;", options.scopes);
      return { success: true };
    },
    login: async (options = {}) => {
      return { code: "mock_zalo_auth_code_12345" };
    },

    // 2. Device & Location
    getLocation: async (options = {}) => {
      return {
        latitude: 10.7769,
        longitude: 106.7009,
        provider: "gps",
      };
    },
    getNetworkType: async () => ({ networkType: "wifi" }),
    getSystemInfo: async () => ({
      platform: "browser_sim",
      system: "macOS / ZMP Simulator",
      version: "1.0.0",
      statusBarHeight: 44,
      windowWidth: 390,
      windowHeight: 844,
      pixelRatio: 3,
    }),

    // 3. Storage
    setStorage: async ({ key, data }) => {
      MOCK_STORAGE.set(key, data);
      return { success: true };
    },
    getStorage: async ({ key }) => {
      const data = MOCK_STORAGE.get(key);
      return { data: data !== undefined ? data : null };
    },
    removeStorage: async ({ key }) => {
      MOCK_STORAGE.delete(key);
      return { success: true };
    },
    clearStorage: async () => {
      MOCK_STORAGE.clear();
      return { success: true };
    },

    // 4. Navigation & Interaction
    openChat: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] openChat()", "color: #0068ff; font-weight: bold;", options);
      alert(`[ZMP Mock] Mở cửa sổ chat: ${JSON.stringify(options)}`);
      return { success: true };
    },
    followOA: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] followOA()", "color: #0068ff; font-weight: bold;", options);
      alert(`[ZMP Mock] Đã quan tâm Official Account: ${options.id || "OA"}`);
      return { success: true };
    },
    showToast: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] showToast()", "color: #0068ff; font-weight: bold;", options.message || options);
      return { success: true };
    },
    createOrder: async (options = {}) => {
      console.log("%c[ZMP SDK Mock] createOrder()", "color: #0068ff; font-weight: bold;", options);
      return { orderId: "zmp_mock_order_" + Date.now(), success: true };
    },
    closeApp: async () => {
      console.log("%c[ZMP SDK Mock] closeApp()", "color: #0068ff; font-weight: bold;");
      return { success: true };
    },
  };

  // Expose as global ZMP
  window.ZMP = window.ZMP || ZmpNativeBridge;
  window.zmp = window.zmp || ZmpNativeBridge;
  window.__ZMP_SIMULATOR__ = true;
})();
"#
    }
}
