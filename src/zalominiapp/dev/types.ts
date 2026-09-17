export interface ZmpAppConfig {
  app?: {
    title?: string;
    headerTitle?: string;
    statusBar?: string;
    actionBar?: {
      title?: string;
      backgroundColor?: string;
      textColor?: string;
      customColor?: string;
      leftButton?: "back" | "none";
    };
    textColor?: "white" | "black";
    leftButton?: "back" | "none";
    hideAndroidBottomNavigationBar?: boolean;
    hideIOSSafeAreaBottom?: boolean;
  };
  listCSS?: Array<{ src: string }>;
  listJS?: Array<{ src: string; type?: string; async?: boolean; id?: string }>;
  pages?: string[];
  [key: string]: any;
}

export interface ZmpDevOptions {
  cwd?: string;
  port?: number | string;
  appId?: string;
  host?: string;
  ios?: boolean;
  iosHostName?: string;
  deviceMode?: boolean;
  remoteDebug?: boolean;
  frame?: boolean;
  autoOpen?: boolean;
  engine?: "rust" | "bun";
}

export interface ZmpDevContext {
  projectDir: string;
  appId: string;
  port: number;
  host: string;
  localIp: string;
  networkUrl: string;
  deepLinkUrl: string;
  config: ZmpAppConfig;
}
