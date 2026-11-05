import { getGlobalConfig } from "./config";
import type { StaticPrepared } from "./static-build";

/** How long `com save` waits for the cloud to answer one request. Override with COM_SAVE_TIMEOUT_MS. */
export function saveTimeoutMs(): number {
  const v = Number(process.env.COM_SAVE_TIMEOUT_MS);
  return Number.isFinite(v) && v > 0 ? v : 180_000;
}

export interface ViewRawFilesResponse {
  id: number;
  /** Server version of the files; sent back as `baseUpdatedAt` on save. */
  updated_at?: string;
  files: Array<{
    path: string;
    content: string;
  }>;
}

export interface ViewDetailsResponse {
  id?: number;
  name?: string;
  description?: string;
  code?: string;
  db_code?: string;
  type?: string;
  status?: string;
  /** Server version of the files; publishing bumps it too. */
  updated_at?: string;
  domains?: string[];
  files?: Array<{
    path: string;
    content?: string;
    code?: string;
  }>;
}

export interface SaveViewOptions {
  name?: string;
  description?: string;
  /** "merge" keeps server files that are not sent; omitted means the server replaces every file. */
  mode?: "merge";
  /** Paths to remove in merge mode. */
  deleted?: string[];
  /** Rejects the save with 409 when the server changed since this version. */
  baseUpdatedAt?: string;
  force?: boolean;
}

export interface SaveViewResponse {
  success: boolean;
  id: number;
  files_count?: number;
  updated_at?: string;
  /** Echoed by servers that support merge saves. */
  mode?: "merge" | "replace";
  revision_id?: number | null;
  ws6_snapshot_revision_id?: number | null;
  realtime?: "reset" | "unavailable" | "disabled";
}

/** The view changed on the server since `baseUpdatedAt`. */
export class SaveConflictError extends Error {
  constructor(public readonly updatedAt?: string) {
    super("View was changed on the server since your last clone/save");
    this.name = "SaveConflictError";
  }
}

export class NataApiClient {
  private apiUrl: string;
  private token?: string;

  constructor(apiUrl?: string, token?: string) {
    const config = getGlobalConfig();
    this.apiUrl = (apiUrl || config.apiUrl || "https://base.myworkbeast.com").replace(/\/$/, "");
    this.token = token || config.token;
  }

  get baseUrl(): string {
    return this.apiUrl;
  }

  get hasToken(): boolean {
    return !!this.token;
  }

  private getHeaders(): HeadersInit {
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
    };
    if (this.token) {
      headers["Authorization"] = `Bearer ${this.token}`;
      headers["x-base-key"] = this.token;
    }
    return headers;
  }

  async getViewRawFiles(viewId: number | string): Promise<ViewRawFilesResponse> {
    const url = `${this.apiUrl}/api/views/${viewId}/raw_files`;
    const res = await fetch(url, {
      method: "GET",
      headers: this.getHeaders(),
    });

    if (!res.ok) {
      // Fallback: Thử endpoint /api/views/:idOrDomain
      return await this.getViewFromDetails(viewId);
    }

    const data = await res.json();
    return data as ViewRawFilesResponse;
  }

  async getViewDetails(viewId: number | string): Promise<ViewDetailsResponse> {
    const url = `${this.apiUrl}/api/views/${viewId}`;
    const res = await fetch(url, {
      method: "GET",
      headers: this.getHeaders(),
    });

    if (!res.ok) {
      const errorText = await res.text();
      throw new Error(`Failed to fetch view details (${res.status}): ${errorText}`);
    }

    return await res.json();
  }

  private async getViewFromDetails(viewId: number | string): Promise<ViewRawFilesResponse> {
    const view = await this.getViewDetails(viewId);
    let files: Array<{ path: string; content: string }> = [];

    if (Array.isArray(view.files)) {
      files = view.files.map((f) => ({
        path: f.path,
        content: f.content ?? f.code ?? "",
      }));
    } else if (view.code) {
      try {
        const parsed = JSON.parse(view.code);
        const rawFiles = parsed.files || parsed;
        if (Array.isArray(rawFiles)) {
          files = rawFiles.map((f: any) => ({
            path: f.path,
            content: f.content ?? f.code ?? "",
          }));
        }
      } catch {
        // Single file code fallback
        files = [{ path: "./index.tsx", content: view.code }];
      }
    }

    return {
      id: Number(view.id || viewId),
      files,
    };
  }

  async saveViewRawFiles(
    viewId: number | string,
    files: Array<{ path: string; content: string; es5?: string }>,
    options: SaveViewOptions = {}
  ): Promise<SaveViewResponse> {
    const timeoutMs = saveTimeoutMs();
    const post = (url: string, method: string, body: string) =>
      fetch(url, { method, headers: this.getHeaders(), body, signal: AbortSignal.timeout(timeoutMs) }).catch((err) => {
        if (err?.name === "TimeoutError" || err?.name === "AbortError") {
          throw new Error(
            `The cloud did not answer within ${Math.round(timeoutMs / 1000)}s (${(body.length / 1e6).toFixed(1)} MB sent to ${url}). ` +
              "The server may be busy or rejecting large uploads; try again, or set COM_SAVE_TIMEOUT_MS to wait longer."
          );
        }
        throw err;
      });
    const rawFilesUrl = `${this.apiUrl}/api/views/${viewId}/raw_files`;
    const payload = {
      files,
      name: options.name,
      description: options.description,
      code: JSON.stringify({ files }),
      mode: options.mode,
      deleted: options.deleted,
      base_updated_at: options.baseUpdatedAt,
      force: options.force,
    };

    const body = JSON.stringify(payload);

    // 1. Try POST /api/views/:id/raw_files
    let res = await post(rawFilesUrl, "POST", body);

    // 2. If endpoint not found (404/405), fallback to POST /api/views/:id/save
    if (!res.ok && (res.status === 404 || res.status === 405)) {
      res = await post(`${this.apiUrl}/api/views/${viewId}/save`, "POST", body);
    }

    // 3. Fallback to standard PUT /api/views/:id
    if (!res.ok && (res.status === 404 || res.status === 405)) {
      res = await post(`${this.apiUrl}/api/views/${viewId}`, "PUT", body);
    }

    if (res.status === 409) {
      const body = await res.json().catch(() => ({}));
      throw new SaveConflictError(body?.updated_at);
    }

    if (!res.ok) {
      const errorText = await res.text();
      throw new Error(`Failed to save view to cloud (${res.status}): ${errorText}`);
    }

    return await res.json();
  }

  /**
   * `com publish` is the static publish, the same as the web's "Xuất bản tĩnh" and the COM app:
   * the app is built on this machine (see static-build.ts) and the server only stores the files.
   * There is no fallback to the old publishes (V1/V2).
   *
   * Step 1: the view's files, package versions and the builder to run (`unchanged` when the
   * published app already has this source).
   */
  async prepareStaticPublish(viewId: number | string): Promise<StaticPrepared> {
    const res = await fetch(`${this.apiUrl}/api/views/${viewId}/publish-static/browser`, {
      headers: this.getHeaders(),
    });
    if (!res.ok) {
      throw new Error(`Failed to prepare the publish (${res.status}): ${await res.text()}`);
    }
    return await res.json();
  }

  /** Step 2: stores the built files. 409: the source changed since the prepare, build again. */
  async uploadStaticPublish(viewId: number | string, payload: unknown): Promise<{ status: "conflict" } | { status: "ok"; body: any }> {
    const res = await fetch(`${this.apiUrl}/api/views/${viewId}/publish-static/browser`, {
      method: "POST",
      headers: this.getHeaders(),
      body: JSON.stringify(payload),
    });
    if (res.status === 409) return { status: "conflict" };
    if (!res.ok) {
      throw new Error(`Failed to publish view (${res.status}): ${await res.text()}`);
    }
    return { status: "ok", body: await res.json().catch(() => ({})) };
  }
}
