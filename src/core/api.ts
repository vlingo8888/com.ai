import { getGlobalConfig } from "./config";

export interface ViewRawFilesResponse {
  id: number;
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
  domains?: string[];
  files?: Array<{
    path: string;
    content?: string;
    code?: string;
  }>;
}

export class NataApiClient {
  private apiUrl: string;
  private token?: string;

  constructor(apiUrl?: string, token?: string) {
    const config = getGlobalConfig();
    this.apiUrl = (apiUrl || config.apiUrl || "https://base.myworkbeast.com").replace(/\/$/, "");
    this.token = token || config.token;
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
}
