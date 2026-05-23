import "./App.css";
import { TooltipProvider } from "@/components/ui/tooltip";

function App() {
  return (
    <TooltipProvider>
      <main className="container">Rustysend</main>
    </TooltipProvider>
  );
}

export default App;
