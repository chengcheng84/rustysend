import { useState } from "react";
import { Switch } from "@/components/ui/switch";

export function SettingsPage() {
  const [autoReceive, setAutoReceive] = useState(true);

  return (
    <div className="space-y-6">
      <div className="rounded-xl border border-border bg-card p-6">
        <h2 className="text-base font-medium text-card-foreground mb-4">
          通用设置
        </h2>
        <div className="space-y-4">
          <div className="flex items-center justify-between py-2">
            <div>
              <p className="text-sm font-medium text-foreground">
                自动接收文件
              </p>
              <p className="text-xs text-muted-foreground">
                自动接受来自信任设备的文件
              </p>
            </div>
            <Switch
              checked={autoReceive}
              onCheckedChange={setAutoReceive}
            />
          </div>
          <div className="flex items-center justify-between py-2">
            <div>
              <p className="text-sm font-medium text-foreground">下载路径</p>
              <p className="text-xs text-muted-foreground">
                ~/Downloads/RustySend
              </p>
            </div>
            <button className="rounded-md border border-border bg-background px-3 py-1.5 text-xs font-medium text-foreground hover:bg-accent">
              更改
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
