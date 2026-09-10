import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useState, type PropsWithChildren } from "react";

import {
  ENVIRONMENT_PROBING_RETRY_DELAY_MS,
  retryWhileEnvironmentProbing,
} from "@/lib/rpc";

export function AppProviders({ children }: PropsWithChildren) {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            // 启动期后端还在后台探测工具；对 ENVIRONMENT_PROBING 静默重试，
            // 让依赖环境的查询保持 pending 而不是闪一次错误态。
            retry: retryWhileEnvironmentProbing,
            retryDelay: ENVIRONMENT_PROBING_RETRY_DELAY_MS,
            refetchOnWindowFocus: false,
          },
        },
      }),
  );

  return (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  );
}
