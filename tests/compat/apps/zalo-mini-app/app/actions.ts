"use server";

export async function whoAmI() {
  return `Action ran on the ${typeof window === "undefined" ? "server" : "device"}`;
}
