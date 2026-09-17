// Modern ANSI styling & design system for COM.AI.VN CLI (English)

export const colors = {
  reset: "\x1b[0m",
  bold: "\x1b[1m",
  dim: "\x1b[2m",
  italic: "\x1b[3m",
  underline: "\x1b[4m",
  strikethrough: "\x1b[9m",

  // Foreground colors
  black: "\x1b[30m",
  red: "\x1b[38;2;248;113;113m",     // #f87171
  green: "\x1b[38;2;52;211;153m",    // #34d399
  emerald: "\x1b[38;2;16;185;129m",  // #10b981
  yellow: "\x1b[38;2;251;191;36m",   // #fbbf24
  blue: "\x1b[38;2;96;165;250m",     // #60a5fa
  indigo: "\x1b[38;2;129;140;248m",  // #818cf8
  magenta: "\x1b[38;2;192;132;252m", // #c084fc
  cyan: "\x1b[38;2;56;189;248m",     // #38bdf8
  sky: "\x1b[38;2;14;165;233m",      // #0ea5e9
  white: "\x1b[37m",
  gray: "\x1b[38;2;100;116;139m",    // #64748b
  darkGray: "\x1b[38;2;71;85;105m",  // #475569

  // Background colors
  bgCyan: "\x1b[48;2;14;165;233m",
  bgIndigo: "\x1b[48;2;79;70;229m",
  bgEmerald: "\x1b[48;2;16;185;129m",
  bgDark: "\x1b[48;2;15;23;42m",
};

export class Spinner {
  private timer: any = null;
  private frameIndex = 0;
  private readonly frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
  private startTime: number = Date.now();

  constructor(private message: string) {}

  start(msg?: string): Spinner {
    if (msg) this.message = msg;
    this.startTime = Date.now();
    process.stdout.write("\x1b[?25l"); // Hide cursor

    this.timer = setInterval(() => {
      const frame = this.frames[this.frameIndex++ % this.frames.length];
      const elapsed = ((Date.now() - this.startTime) / 1000).toFixed(1);
      process.stdout.write(
        `\r  ${colors.cyan}${frame}${colors.reset} ${this.message} ${colors.darkGray}(${elapsed}s)${colors.reset}\x1b[K`
      );
    }, 80);

    return this;
  }

  update(msg: string): void {
    this.message = msg;
  }

  stop(success = true, finalMsg?: string): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
    process.stdout.write("\x1b[?25h"); // Restore cursor

    const duration = ((Date.now() - this.startTime) / 1000).toFixed(2);
    const icon = success ? `${colors.green}✔${colors.reset}` : `${colors.red}✖${colors.reset}`;
    const text = finalMsg || this.message;
    const timeStr = `${colors.darkGray}${duration}s${colors.reset}`;

    process.stdout.write(`\r  ${icon} ${text}  ${timeStr}\n\x1b[K`);
  }
}

export const logger = {
  spinner(msg: string): Spinner {
    return new Spinner(msg).start();
  },

  hero() {
    console.log(`
${colors.cyan}  ┌────────────────────────────────────────────────────────┐
  │  ${colors.bold}${colors.indigo}◆ COM.AI.VN CLI${colors.reset}${colors.cyan}                                      │
  │  ${colors.darkGray}The Intelligent Web & Cloud Engine Developer Tooling${colors.cyan}   │
  └────────────────────────────────────────────────────────┘${colors.reset}`);
  },

  section(title: string) {
    console.log(`\n ${colors.bgCyan} ${colors.bold}\x1b[30m ${title.toUpperCase()} \x1b[0m\n`);
  },

  step(current: number, total: number, title: string) {
    console.log(`\n ${colors.bold}${colors.cyan}[${current}/${total}]${colors.reset} ${colors.bold}${title}${colors.reset}`);
  },

  info(msg: string) {
    console.log(`  ${colors.blue}ℹ${colors.reset} ${msg}`);
  },

  success(msg: string) {
    console.log(`  ${colors.green}✔${colors.reset} ${colors.bold}${msg}${colors.reset}`);
  },

  warn(msg: string) {
    console.log(`  ${colors.yellow}▲${colors.reset} ${colors.yellow}${msg}${colors.reset}`);
  },

  error(msg: string, detail?: string) {
    console.error(`\n  ${colors.red}✖ ${colors.bold}ERROR:${colors.reset} ${colors.red}${msg}${colors.reset}`);
    if (detail) {
      console.error(`    ${colors.darkGray}${detail}${colors.reset}`);
    }
    console.log();
  },

  fileItem(path: string, size?: number, isLast = false) {
    const branch = isLast ? "└──" : "├──";
    const sizeStr = size !== undefined ? `${colors.darkGray}(${(size / 1024).toFixed(1)} KB)${colors.reset}` : "";
    console.log(`    ${colors.darkGray}${branch}${colors.reset} ${colors.green}${path}${colors.reset} ${sizeStr}`);
  },

  card(title: string, items: Array<{ label: string; value: string; color?: string }>) {
    const maxLabelLen = Math.max(...items.map((i) => i.label.length), 10);
    console.log(`\n  ${colors.bold}${colors.indigo}┌─ ${title} ${"─".repeat(Math.max(40 - title.length, 4))}┐${colors.reset}`);

    for (const item of items) {
      const pad = " ".repeat(maxLabelLen - item.label.length);
      const valColor = item.color || colors.cyan;
      console.log(`  ${colors.indigo}│${colors.reset}  ${colors.darkGray}${item.label}:${pad}${colors.reset} ${valColor}${item.value}${colors.reset}`);
    }

    console.log(`  ${colors.indigo}└${"─".repeat(47)}┘${colors.reset}\n`);
  },

  nextSteps(commands: Array<{ cmd: string; desc: string }>) {
    console.log(`  ${colors.bold}${colors.green}✨ NEXT STEPS:${colors.reset}`);
    for (const { cmd, desc } of commands) {
      console.log(`    ${colors.darkGray}›${colors.reset} ${colors.bold}${colors.cyan}${cmd}${colors.reset} ${colors.darkGray}# ${desc}${colors.reset}`);
    }
    console.log();
  }
};
