import { Monitor, ArrowLeftRight, Settings } from "lucide-react";
import type { LucideIcon } from "lucide-react";

export interface NavItem {
  id: string;
  path: string;
  icon: LucideIcon;
  label: string;
}

export const NAV_ITEMS: NavItem[] = [
  { id: "devices", path: "/devices", icon: Monitor, label: "设备" },
  { id: "transfer", path: "/transfer", icon: ArrowLeftRight, label: "传输" },
  { id: "settings", path: "/settings", icon: Settings, label: "设置" },
];
