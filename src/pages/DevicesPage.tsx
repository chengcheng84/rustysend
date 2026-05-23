import { Monitor, Smartphone, Tablet, Loader2 } from "lucide-react";
import { useDevices } from "@/hooks/useDevices";
import type { Device } from "@/types/device";

function DeviceIcon({ type }: { type: Device["type"] }) {
  switch (type) {
    case "mobile":
      return <Smartphone className="h-5 w-5 text-primary" />;
    case "tablet":
      return <Tablet className="h-5 w-5 text-primary" />;
    default:
      return <Monitor className="h-5 w-5 text-primary" />;
  }
}

export function DevicesPage() {
  const { devices, isLoading, error } = useDevices();

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center">
        <Loader2 className="h-8 w-8 animate-spin text-primary" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-sm text-destructive">加载设备列表失败</p>
      </div>
    );
  }

  return (
    <div className="space-y-6 p-6">
      <div className="rounded-xl border border-border bg-card p-6">
        <h2 className="text-base font-medium text-card-foreground mb-4">
          已发现的设备
        </h2>
        <div className="space-y-3">
          {devices.map((device) => (
            <div
              key={device.id}
              className="flex items-center gap-4 rounded-lg border border-border bg-background p-4"
            >
              <div className="flex h-10 w-10 items-center justify-center rounded-full bg-primary/10">
                <DeviceIcon type={device.type} />
              </div>
              <div className="flex-1">
                <p className="text-sm font-medium text-foreground">
                  {device.name}
                </p>
                <p className="text-xs text-muted-foreground">{device.ip}</p>
              </div>
              <div
                className={`flex h-2 w-2 rounded-full ${
                  device.isOnline ? "bg-green-500" : "bg-gray-400"
                }`}
              />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
