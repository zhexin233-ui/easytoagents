import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  fireEvent,
  render,
  type RenderOptions,
  type RenderResult,
} from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import type { ReactNode } from "react";

import { NotifyProvider } from "@/components/notify";

type User = {
  click: (element: Element) => Promise<boolean>;
  type: (element: Element, text: string) => Promise<boolean>;
};

export type RenderWithProvidersOptions = RenderOptions & {
  route?: string;
  initialEntries?: string[];
  queryClient?: QueryClient;
  path?: string;
};

export type RenderWithProvidersResult = RenderResult & {
  queryClient: QueryClient;
  user: User;
};

function createTestQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, refetchOnWindowFocus: false },
      mutations: { retry: false },
    },
  });
}

export function renderWithProviders(
  ui: ReactNode,
  options: RenderWithProvidersOptions = {},
): RenderWithProvidersResult {
  const {
    route = "/",
    initialEntries = [route],
    queryClient = createTestQueryClient(),
    path,
    ...renderOptions
  } = options;
  const content = path ? (
    <Routes>
      <Route path={path} element={ui} />
    </Routes>
  ) : (
    ui
  );
  const result = render(
    <QueryClientProvider client={queryClient}>
      <NotifyProvider>
        <MemoryRouter initialEntries={initialEntries}>{content}</MemoryRouter>
      </NotifyProvider>
    </QueryClientProvider>,
    renderOptions,
  );
  const user: User = {
    click: (element) => Promise.resolve(fireEvent.click(element)),
    type: (element, text) =>
      Promise.resolve(fireEvent.change(element, { target: { value: text } })),
  };
  return { ...result, queryClient, user };
}
