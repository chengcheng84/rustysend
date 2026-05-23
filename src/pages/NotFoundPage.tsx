import { Link } from "react-router-dom";
import { Home, AlertCircle } from "lucide-react";

export function NotFoundPage() {
  return (
    <div className="flex h-full flex-col items-center justify-center p-6">
      <div className="flex h-20 w-20 items-center justify-center rounded-full bg-muted">
        <AlertCircle className="h-10 w-10 text-muted-foreground" />
      </div>
      <h1 className="mt-6 text-2xl font-semibold text-foreground">
        页面未找到
      </h1>
      <p className="mt-2 text-center text-sm text-muted-foreground">
        您访问的页面不存在或已被移除
      </p>
      <Link
        to="/devices"
        className="mt-6 inline-flex items-center gap-2 rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90"
      >
        <Home className="h-4 w-4" />
        返回首页
      </Link>
    </div>
  );
}
