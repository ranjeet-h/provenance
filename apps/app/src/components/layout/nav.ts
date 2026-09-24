import {
  LayoutDashboard,
  Users,
  Library,
  Settings,
  type LucideIcon,
} from "lucide-react";

export interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
  end: boolean;
}

export const NAV_ITEMS: readonly NavItem[] = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard, end: true },
  { to: "/sessions", label: "Sessions", icon: Users, end: false },
  { to: "/reference-libraries", label: "Reference Libraries", icon: Library, end: false },
  { to: "/settings", label: "Settings", icon: Settings, end: false },
];
