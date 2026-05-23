import { Routes, Route, Navigate } from "react-router-dom";
import { DevicesPage } from "@/pages/DevicesPage";
import { TransferPage } from "@/pages/TransferPage";
import { SettingsPage } from "@/pages/SettingsPage";

export function AppRoutes() {
  return (
    <Routes>
      <Route path="/" element={<Navigate to="/devices" replace />} />
      <Route path="/devices" element={<DevicesPage />} />
      <Route path="/transfer" element={<TransferPage />} />
      <Route path="/settings" element={<SettingsPage />} />
    </Routes>
  );
}
