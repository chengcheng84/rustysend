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
import { Monitor, ArrowLeftRight, Settings, Circle } from "lucide-react";
import { useState } from "react";

const navItems = [
  { id: "devices", path: "/devices", icon: Monitor, label: "设备" },
  { id: "transfer", path: "/transfer", icon: ArrowLeftRight, label: "传输" },
  { id: "settings", path: "/settings", icon: Settings, label: "设置" },
];

export function AppSidebar() {
  const [isOnline] = useState(true);

  return (
    <Sidebar collapsible="icon" className="border-r border-sidebar-border">
      <SidebarHeader className="flex items-center justify-center py-3">
        <span className="mt-2 text-md font-semibold text-sidebar-foreground group-data-[collapsible=icon]:hidden">
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
                    className="h-12 justify-start gap-3 px-3 text-sidebar-foreground/80 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground data-[active=true]:bg-sidebar-accent data-[active=true]:text-sidebar-accent-foreground data-[active=true]:font-medium"
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
        <div className="flex items-center justify-center gap-2 group-data-[collapsible=icon]:flex-col">
          <div className="flex items-center gap-2">
            <Circle
              className={`h-2.5 w-2.5 fill-current ${
                isOnline ? "text-green-500 animate-pulse" : "text-red-500"
              }`}
            />
            <span className="text-xs text-sidebar-foreground/70 group-data-[collapsible=icon]:hidden">
              {isOnline ? "在线" : "离线"}
            </span>
          </div>
          <span className="text-[10px] text-sidebar-foreground/50 group-data-[collapsible=icon]:hidden">
            v0.1.0
          </span>
        </div>
      </SidebarFooter>
    </Sidebar>
  );
}
