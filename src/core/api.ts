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

  async saveViewRawFiles(
    viewId: number | string,
    files: Array<{ path: string; content: string }>,
    options: { name?: string; description?: string } = {}
  ): Promise<{ success: boolean; id: number; files_count?: number }> {
    const rawFilesUrl = `${this.apiUrl}/api/views/${viewId}/raw_files`;
    const payload = {
      files,
      name: options.name,
      description: options.description,
      code: JSON.stringify({ files }),
    };

    // 1. Try POST /api/views/:id/raw_files
    let res = await fetch(rawFilesUrl, {
      method: "POST",
      headers: this.getHeaders(),
      body: JSON.stringify(payload),
    });

    // 2. If endpoint not found (404/405), fallback to POST /api/views/:id/save
    if (!res.ok && (res.status === 404 || res.status === 405)) {
      const saveUrl = `${this.apiUrl}/api/views/${viewId}/save`;
      res = await fetch(saveUrl, {
        method: "POST",
        headers: this.getHeaders(),
        body: JSON.stringify(payload),
      });
    }

    // 3. Fallback to standard PUT /api/views/:id
    if (!res.ok && (res.status === 404 || res.status === 405)) {
      const updateUrl = `${this.apiUrl}/api/views/${viewId}`;
      res = await fetch(updateUrl, {
        method: "PUT",
        headers: this.getHeaders(),
        body: JSON.stringify(payload),
      });
    }

    if (!res.ok) {
      const errorText = await res.text();
      throw new Error(`Failed to save view to cloud (${res.status}): ${errorText}`);
    }

    return await res.json();
  }

  async publishView(
    viewId: number | string,
    options: { domain?: string } = {}
  ): Promise<{ success: boolean; viewId: number; message?: string }> {
    const publishUrl = `${this.apiUrl}/api/views/publish/v2`;
    const payload = {
      viewId: Number(viewId),
      view_id: Number(viewId),
      domain: options.domain,
    };

    // 1. Try POST /api/views/publish/v2
    let res = await fetch(publishUrl, {
      method: "POST",
      headers: this.getHeaders(),
      body: JSON.stringify(payload),
    });

    // 2. Fallback to POST /api/views/publish
    if (!res.ok && (res.status === 404 || res.status === 405)) {
      const fallbackUrl = `${this.apiUrl}/api/views/publish`;
      res = await fetch(fallbackUrl, {
        method: "POST",
        headers: this.getHeaders(),
        body: JSON.stringify(payload),
      });
    }

    // 3. Fallback to POST /api/views/:id/publish
    if (!res.ok && (res.status === 404 || res.status === 405)) {
      const idUrl = `${this.apiUrl}/api/views/${viewId}/publish`;
      res = await fetch(idUrl, {
        method: "POST",
        headers: this.getHeaders(),
        body: JSON.stringify(payload),
      });
    }

    if (!res.ok) {
      const errorText = await res.text();
      throw new Error(`Failed to publish view (${res.status}): ${errorText}`);
    }

    return await res.json();
  }
}
