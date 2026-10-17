import { ImageResponse } from "next/og";

export async function GET() {
  return new ImageResponse(
    <div style={{ width: "100%", height: "100%", display: "flex", alignItems: "center", justifyContent: "center", background: "#0068ff", color: "white", fontSize: 64 }}>
      Com.AI.VN
    </div>,
    { width: 600, height: 315 },
  );
}
