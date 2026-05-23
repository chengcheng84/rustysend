import "./App.css";
import { BrowserRouter } from "react-router-dom";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SidebarProvider } from "@/components/ui/sidebar";
import { AppSidebar } from "@/components/AppSidebar";
import { AppRoutes } from "@/routes";

function App() {
  return (
    <BrowserRouter>
      <TooltipProvider>
        <SidebarProvider defaultOpen={true}>
          <div className="flex h-screen w-full overflow-hidden">
            <AppSidebar />
            <main className="flex-1 flex flex-col bg-background overflow-auto">
              <AppRoutes />
            </main>
          </div>
        </SidebarProvider>
      </TooltipProvider>
    </BrowserRouter>
  );
}

export default App;
