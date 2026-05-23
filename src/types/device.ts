export interface Device {
  id: string;
  name: string;
  ip: string;
  isOnline: boolean;
  type: "desktop" | "mobile" | "tablet";
}
