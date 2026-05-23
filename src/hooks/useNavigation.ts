import { useMemo } from "react";
import { NAV_ITEMS, type NavItem } from "@/config/navigation";

export function useNavigation(): NavItem[] {
  // 未来可以在这里添加动态菜单逻辑
  // 例如：根据用户权限过滤、从后端获取菜单等
  return useMemo(() => NAV_ITEMS, []);
}

export function useActiveNavItem(pathname: string): NavItem | undefined {
  const navItems = useNavigation();
  return useMemo(
    () => navItems.find((item) => pathname.startsWith(item.path)),
    [navItems, pathname]
  );
}
