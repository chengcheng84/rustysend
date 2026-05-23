import { useState, useEffect } from "react";
import type { Device } from "@/types/device";

// Mock 数据 - 未来替换为真实 API 调用
const mockDevices: Device[] = [
  {
    id: "1",
    name: "MacBook Pro",
    ip: "192.168.1.100",
    isOnline: true,
    type: "desktop",
  },
  {
    id: "2",
    name: "iPhone 15",
    ip: "192.168.1.101",
    isOnline: true,
    type: "mobile",
  },
];

export function useDevices() {
  const [devices, setDevices] = useState<Device[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    // 模拟 API 调用延迟
    const fetchDevices = async () => {
      try {
        setIsLoading(true);
        // 未来替换为: const data = await invoke('get_devices')
        await new Promise((resolve) => setTimeout(resolve, 500));
        setDevices(mockDevices);
      } catch (err) {
        setError(err instanceof Error ? err : new Error("Failed to fetch devices"));
      } finally {
        setIsLoading(false);
      }
    };

    fetchDevices();
  }, []);

  return { devices, isLoading, error };
}
