import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { NavLink, Outlet, useLocation } from "react-router-dom";

import claudeIconUrl from "@/assets/brand/claude-icon-square.svg";
import codexIconUrl from "@/assets/brand/codex-icon-light.png";
import { projectsQueryOptions } from "@/lib/projects-api";
import { cn } from "@/lib/utils";

const primaryLinks = [
  { to: "/", label: "总览", end: true },
  { to: "/mcp", label: "MCP", end: false },
  { to: "/skills", label: "Skills", end: false },
] as const;

const toolLinks = [
  { to: "/claude", label: "Claude", icon: claudeIconUrl },
  { to: "/codex", label: "Codex", icon: codexIconUrl },
] as const;

const primaryLinkClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    "flex-1 rounded-md px-3 py-2 text-sm font-medium transition-colors",
    isActive
      ? "bg-primary text-primary-foreground"
      : "text-muted-foreground hover:bg-muted hover:text-foreground",
  );

export function AppShell() {
  const [projectsExpanded, setProjectsExpanded] = useState(true);
  const { pathname } = useLocation();
  const projectSectionOpen =
    projectsExpanded || pathname.startsWith("/projects/");

  return (
    <div className="flex h-screen flex-col overflow-hidden">
      <TopBar />
      <div className="flex min-h-0 flex-1">
        <aside className="flex w-60 shrink-0 flex-col border-r bg-white">
          <nav
            aria-label="一级导航"
            className="min-h-0 flex-1 space-y-1 overflow-y-auto px-3 py-4"
          >
            {primaryLinks.map((link) => (
              <div key={link.to} className="flex">
                <NavLink
                  to={link.to}
                  end={link.end}
                  className={primaryLinkClass}
                >
                  {link.label}
                </NavLink>
              </div>
            ))}
            <ProjectNavSection
              open={projectSectionOpen}
              onToggle={() => setProjectsExpanded((expanded) => !expanded)}
              onNavigate={() => setProjectsExpanded(true)}
            />
          </nav>
        </aside>
        <div className="min-w-0 flex-1 overflow-y-auto">
          <Outlet />
        </div>
      </div>
    </div>
  );
}

function TopBar() {
  return (
    <header className="flex h-14 shrink-0 items-center justify-between gap-4 border-b bg-white px-4 lg:px-6">
      <div className="flex items-center gap-2.5">
        <span
          aria-hidden="true"
          className="bg-primary text-primary-foreground flex size-7 items-center justify-center rounded-lg text-xs font-bold"
        >
          EA
        </span>
        <div className="leading-tight">
          <p className="text-sm font-semibold">EasyToAgents</p>
          <p className="text-muted-foreground text-[11px]">
            Claude · Codex 配置中枢
          </p>
        </div>
      </div>
      <nav aria-label="工具入口" className="flex items-center gap-1.5">
        {toolLinks.map((link) => (
          <NavLink
            key={link.to}
            to={link.to}
            className={({ isActive }) =>
              cn(
                "flex items-center gap-2 rounded-full border px-3 py-1.5 text-sm font-medium transition-colors",
                isActive
                  ? "bg-primary text-primary-foreground border-transparent"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )
            }
          >
            <img
              src={link.icon}
              alt=""
              aria-hidden="true"
              draggable={false}
              className="size-4 rounded-[4px] object-contain"
            />
            {link.label}
          </NavLink>
        ))}
      </nav>
    </header>
  );
}

interface ProjectNavSectionProps {
  open: boolean;
  onToggle: () => void;
  onNavigate: () => void;
}

function ProjectNavSection({
  open,
  onToggle,
  onNavigate,
}: ProjectNavSectionProps) {
  const projectsQuery = useQuery(projectsQueryOptions());
  const projects = projectsQuery.data ?? [];

  return (
    <div>
      <div className="flex items-center">
        <NavLink
          to="/projects"
          end={false}
          className={primaryLinkClass}
          onClick={onNavigate}
        >
          项目
        </NavLink>
        <button
          type="button"
          aria-expanded={open}
          aria-label={open ? "收起项目列表" : "展开项目列表"}
          title={open ? "收起项目列表" : "展开项目列表"}
          className="text-muted-foreground hover:bg-muted hover:text-foreground mr-1 flex size-6 shrink-0 items-center justify-center rounded transition-colors"
          onClick={onToggle}
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            className={cn("size-3.5 transition-transform", open && "rotate-90")}
          >
            <path d="M6 3.5 10.5 8 6 12.5" />
          </svg>
        </button>
      </div>
      {open ? (
        <div className="mt-1 ml-3 space-y-0.5 border-l pl-3">
          {projectsQuery.isPending ? (
            <p
              role="status"
              className="text-muted-foreground px-2 py-1 text-xs"
            >
              正在读取项目…
            </p>
          ) : null}
          {projectsQuery.isError ? (
            <p role="alert" className="px-2 py-1 text-xs text-red-700">
              项目列表加载失败
            </p>
          ) : null}
          {!projectsQuery.isPending &&
          !projectsQuery.isError &&
          projects.length === 0 ? (
            <p className="text-muted-foreground px-2 py-1 text-xs">
              暂无已登记项目
            </p>
          ) : null}
          {projects.map((project) => (
            <NavLink
              key={project.id}
              to={`/projects/${project.id}`}
              className={({ isActive }) =>
                cn(
                  "block truncate rounded-md px-2 py-1.5 text-sm transition-colors",
                  isActive
                    ? "bg-muted text-foreground font-medium"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )
              }
              title={project.displayName}
            >
              {project.displayName}
            </NavLink>
          ))}
        </div>
      ) : null}
    </div>
  );
}
