import { NavLink } from "react-router-dom";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";
import { useNavigation } from "@/hooks/useNavigation";

export function AppSidebar() {
  const navItems = useNavigation();

  return (
    <Sidebar collapsible="icon" className="border-r border-sidebar-border">
      <SidebarHeader className="flex items-center justify-center py-3">
        <span className="mt-2 text-sm font-semibold text-sidebar-foreground group-data-[collapsible=icon]:hidden">
          RustySend
        </span>
      </SidebarHeader>

      <SidebarContent className="flex-1">
        <SidebarMenu className="gap-1 px-2">
          {navItems.map((item) => (
            <SidebarMenuItem key={item.id}>
              <NavLink to={item.path} className="block">
                {({ isActive }) => (
                  <SidebarMenuButton
                    isActive={isActive}
                    tooltip={item.label}
                    className="h-10 justify-start gap-3 px-3 text-sidebar-foreground/80 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground data-[active=true]:bg-sidebar-accent data-[active=true]:text-sidebar-accent-foreground data-[active=true]:font-medium"
                  >
                    <item.icon className="h-5 w-5 shrink-0" />
                    <span className="group-data-[collapsible=icon]:hidden">
                      {item.label}
                    </span>
                  </SidebarMenuButton>
                )}
              </NavLink>
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </SidebarContent>

      <SidebarFooter className="border-t border-sidebar-border p-3">
        <span className="text-[10px] text-sidebar-foreground/50 group-data-[collapsible=icon]:hidden">
          v0.1.0
        </span>
      </SidebarFooter>
    </Sidebar>
  );
}
