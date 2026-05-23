import { Monitor } from "lucide-react";

export function DevicesPage() {
  return (
    <div className="space-y-6">
      <div className="rounded-xl border border-border bg-card p-6">
        <h2 className="text-base font-medium text-card-foreground mb-4">
          已发现的设备
        </h2>
        <div className="space-y-3">
          <div className="flex items-center gap-4 rounded-lg border border-border bg-background p-4">
            <div className="flex h-10 w-10 items-center justify-center rounded-full bg-primary/10">
              <Monitor className="h-5 w-5 text-primary" />
            </div>
            <div className="flex-1">
              <p className="text-sm font-medium text-foreground">MacBook Pro</p>
              <p className="text-xs text-muted-foreground">192.168.1.100</p>
            </div>
            <div className="flex h-2 w-2 rounded-full bg-green-500" />
          </div>
          <div className="flex items-center gap-4 rounded-lg border border-border bg-background p-4">
            <div className="flex h-10 w-10 items-center justify-center rounded-full bg-primary/10">
              <Monitor className="h-5 w-5 text-primary" />
            </div>
            <div className="flex-1">
              <p className="text-sm font-medium text-foreground">iPhone 15</p>
              <p className="text-xs text-muted-foreground">192.168.1.101</p>
            </div>
            <div className="flex h-2 w-2 rounded-full bg-green-500" />
          </div>
        </div>
      </div>
    </div>
  );
}
