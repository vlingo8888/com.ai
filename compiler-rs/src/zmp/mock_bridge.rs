//! Zalo Mini App Mock SDK Native Bridge for in-browser simulation

pub struct ZmpMockBridge;

/// When the fake `window.ZMP` is installed (`COM_ZMP_MOCK`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockMode {
    /// Never (default): the app talks to the real `zmp-sdk`
    Off,
    /// Every page (`com dev --zmp-mock`, `COM_ZMP_MOCK=1`)
    Always,
    /// Desktop browsers only (`COM_ZMP_MOCK=browser`): the simulator of COM Studio gets the fake SDK,
    /// while a phone that opened the app in Zalo (scanned QR, same server through the tunnel) keeps the real one
    Browser,
}

impl MockMode {
    pub fn parse(value: Option<&str>) -> MockMode {
        match value.map(str::trim) {
            Some("1" | "true") => MockMode::Always,
            Some("browser") => MockMode::Browser,
            _ => MockMode::Off,
        }
    }
}

impl ZmpMockBridge {
    /// Opt-in only (`com dev --zmp-mock`, i.e. COM_ZMP_MOCK=1, or `browser`): the app talks to the real
    /// `zmp-sdk` by default, and a fake `window.ZMP` would hide what the SDK actually does
    pub fn mode() -> MockMode {
        MockMode::parse(std::env::var("COM_ZMP_MOCK").ok().as_deref())
    }

    pub fn enabled() -> bool {
        Self::mode() != MockMode::Off
    }

    /// `<script>` installing the mock bridge when it is enabled, empty otherwise
    pub fn script_tag() -> String {
        Self::script_tag_for(Self::mode())
    }

    pub fn script_tag_for(mode: MockMode) -> String {
        match mode {
            MockMode::Off => String::new(),
            MockMode::Always => format!("<script>{}</script>", Self::get_browser_mock_script()),
            // Zalo's webview has "Zalo" in its user agent: leave the real native bridge alone there
            MockMode::Browser => format!(
                "<script>if(!/\\bZalo/i.test(navigator.userAgent)){{{}}}</script>",
                Self::get_browser_mock_script()
            ),
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_modes() {
        assert_eq!(MockMode::parse(None), MockMode::Off);
        assert_eq!(MockMode::parse(Some("0")), MockMode::Off);
        assert_eq!(MockMode::parse(Some("1")), MockMode::Always);
        assert_eq!(MockMode::parse(Some("browser")), MockMode::Browser);
        assert_eq!(ZmpMockBridge::script_tag_for(MockMode::Off), "");
        let always = ZmpMockBridge::script_tag_for(MockMode::Always);
        assert!(always.starts_with("<script>") && always.contains("window.ZMP"));
        // Browser: the same bridge, skipped inside Zalo
        let browser = ZmpMockBridge::script_tag_for(MockMode::Browser);
        assert!(browser.starts_with(r"<script>if(!/\bZalo/i.test(navigator.userAgent)){"));
        assert!(browser.contains("window.ZMP") && browser.ends_with("}</script>"));
    }

    #[test]
    fn test_mock_bridge_is_opt_in() {
        if std::env::var("COM_ZMP_MOCK").is_err() {
            assert_eq!(ZmpMockBridge::script_tag(), "");
        }
    }
}
