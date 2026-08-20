import { NavLink, Outlet } from "react-router-dom";

import { cn } from "@/lib/utils";

const links = [
  { to: "/", label: "总览", end: true },
  { to: "/claude", label: "Claude", end: false },
  { to: "/codex", label: "Codex", end: false },
  { to: "/mcp", label: "MCP", end: false },
] as const;

export function AppShell() {
  return (
    <div className="min-h-screen lg:grid lg:grid-cols-[220px_1fr]">
      <aside className="border-b bg-white px-5 py-5 lg:border-r lg:border-b-0">
        <p className="text-sm font-semibold">EasyToAgents</p>
        <nav aria-label="一级导航" className="mt-4 flex gap-2 lg:flex-col">
          {links.map((link) => (
            <NavLink
              key={link.to}
              to={link.to}
              end={link.end}
              className={({ isActive }) =>
                cn(
                  "rounded-md px-3 py-2 text-sm transition-colors",
                  isActive
                    ? "bg-primary text-primary-foreground"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )
              }
            >
              {link.label}
            </NavLink>
          ))}
        </nav>
      </aside>
      <Outlet />
    </div>
  );
}
