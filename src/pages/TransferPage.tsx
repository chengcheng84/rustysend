export function TransferPage() {
  return (
    <div className="space-y-6">
      <div className="rounded-xl border border-border bg-card p-6">
        <h2 className="text-base font-medium text-card-foreground mb-4">
          传输任务
        </h2>
        <div className="space-y-3">
          <div className="rounded-lg border border-border bg-background p-4">
            <div className="flex items-center justify-between mb-2">
              <span className="text-sm font-medium text-foreground">
                document.pdf
              </span>
              <span className="text-xs text-muted-foreground">45%</span>
            </div>
            <div className="h-2 w-full rounded-full bg-secondary">
              <div className="h-2 w-[45%] rounded-full bg-primary transition-all" />
            </div>
            <p className="mt-2 text-xs text-muted-foreground">发送至 MacBook Pro</p>
          </div>
          <div className="rounded-lg border border-border bg-background p-4">
            <div className="flex items-center justify-between mb-2">
              <span className="text-sm font-medium text-foreground">
                image.png
              </span>
              <span className="text-xs text-green-500">完成</span>
            </div>
            <div className="h-2 w-full rounded-full bg-secondary">
              <div className="h-2 w-full rounded-full bg-green-500 transition-all" />
            </div>
            <p className="mt-2 text-xs text-muted-foreground">发送至 iPhone 15</p>
          </div>
        </div>
      </div>
    </div>
  );
}
