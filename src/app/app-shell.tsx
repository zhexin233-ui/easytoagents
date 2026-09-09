import { useId, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Trash2 } from "lucide-react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";

import { commands, type ProjectDto } from "@/bindings/commands";
import { FormDialog } from "@/components/form-dialog";
import { Notify } from "@/components/notify";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { useTheme } from "@/components/use-theme";
import { useNotify } from "@/components/use-notify";
import { Button } from "@/components/ui/button";
import { SettingsDialog } from "@/features/settings/settings-dialog";
import { mcpKeys } from "@/lib/mcp-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { projectKeys, projectsQueryOptions } from "@/lib/projects-api";
import { skillKeys } from "@/lib/skills-api";
import {
  PROFILE_TOOLS,
  filterEnabledTools,
  toolMetadata,
} from "@/lib/tool-metadata";
import { cn } from "@/lib/utils";

const primaryLinks = [
  { to: "/", label: "总览", end: true },
  { to: "/prompts", label: "提示词", end: false },
  { to: "/mcp", label: "MCP", end: false },
  { to: "/hooks", label: "Hooks", end: false },
  { to: "/skills", label: "Skills", end: false },
] as const;

const primaryLinkClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    "flex-1 rounded-md px-3 py-2 text-sm font-medium transition-colors",
    isActive
      ? "bg-primary text-primary-foreground"
      : "text-muted-foreground hover:bg-muted hover:text-foreground",
  );

const projectRowActionClass =
  "text-muted-foreground hover:text-foreground pointer-events-none size-8 shrink-0 border-0 bg-transparent p-0 opacity-0 shadow-none transition-[color,opacity] group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100 hover:bg-transparent focus-visible:pointer-events-auto focus-visible:opacity-100";

export function AppShell() {
  const [projectsExpanded, setProjectsExpanded] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { preference, setPreference } = useTheme();
  const { pathname } = useLocation();
  const projectSectionOpen =
    projectsExpanded || pathname.startsWith("/projects/");

  return (
    <div className="flex h-screen flex-col overflow-hidden">
      <TopBar />
      <div className="flex min-h-0 flex-1">
        <aside className="bg-card flex w-60 shrink-0 flex-col border-r">
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
          <div className="border-t px-3 py-3">
            <button
              type="button"
              aria-haspopup="dialog"
              onClick={() => setSettingsOpen(true)}
              className="text-muted-foreground hover:bg-muted hover:text-foreground flex w-full items-center gap-2 rounded-md px-3 py-2 text-sm font-medium transition-colors"
            >
              <SettingsIcon />
              设置
            </button>
          </div>
        </aside>
        <div className="min-w-0 flex-1 overflow-y-auto">
          <Outlet />
        </div>
      </div>
      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        themePreference={preference}
        onThemePreferenceChange={setPreference}
      />
    </div>
  );
}

function SettingsIcon() {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="size-4 shrink-0"
    >
      <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  );
}

function TopBar() {
  const enabledTools = useEnabledTools();
  const toolLinks = filterEnabledTools(PROFILE_TOOLS, enabledTools).map(
    (tool) => {
      const metadata = toolMetadata(tool);
      return {
        to: metadata.profileRoute,
        label: metadata.label,
        icon: metadata.icon,
      };
    },
  );

  return (
    <header className="bg-card flex h-14 shrink-0 items-center justify-between gap-4 border-b px-4 lg:px-6">
      <div className="flex items-center gap-2.5">
        <span
          aria-hidden="true"
          className="bg-primary text-primary-foreground flex size-7 items-center justify-center rounded-lg text-xs font-bold"
        >
          EA
        </span>
        <div className="leading-tight">
          <p className="text-sm font-semibold">EasyToAgents</p>
          <p className="text-muted-foreground text-[11px]">多工具配置中枢</p>
        </div>
      </div>
      <div className="flex items-center gap-2.5">
        <nav aria-label="工具入口" className="flex items-center gap-1.5">
          {toolLinks.map((link) => (
            <NavLink
              key={link.to}
              to={link.to}
              className={({ isActive }) =>
                cn(
                  "flex items-center gap-2 rounded-full border px-3 py-1.5 text-sm font-medium transition-colors",
                  isActive
                    ? "border-primary-foreground bg-primary text-primary-foreground shadow-sm"
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
      </div>
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
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const projectsQuery = useQuery(projectsQueryOptions());
  const [renameDialogProject, setRenameDialogProject] =
    useState<ProjectDto | null>(null);
  const [renameDisplayName, setRenameDisplayName] = useState("");
  const [renameSubmitting, setRenameSubmitting] = useState(false);
  const renameInFlightRef = useRef(false);
  const [removeDialogProject, setRemoveDialogProject] =
    useState<ProjectDto | null>(null);
  const [removeSubmitting, setRemoveSubmitting] = useState(false);
  const removeInFlightRef = useRef(false);
  const { notification, notify } = useNotify();
  const projects = projectsQuery.data ?? [];
  const renameMutation = useMutation({
    mutationFn: async () => {
      if (!renameDialogProject) {
        throw new Error("缺少待编辑项目");
      }
      return unwrapResult(
        await commands.renameProject({
          id: renameDialogProject.id,
          displayName: renameDisplayName,
          rowVersion: renameDialogProject.rowVersion,
        }),
      );
    },
    onSuccess: async (updated) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: projectKeys.all }),
        queryClient.invalidateQueries({ queryKey: mcpKeys.projects() }),
        queryClient.invalidateQueries({ queryKey: skillKeys.projects() }),
      ]);
      renameInFlightRef.current = false;
      setRenameSubmitting(false);
      setRenameDialogProject(null);
      setRenameDisplayName("");
      notify({
        kind: "success",
        message: `项目显示名称已修改为“${updated.displayName}”。`,
      });
    },
    onError: () => {
      renameInFlightRef.current = false;
      setRenameSubmitting(false);
    },
  });
  const removeMutation = useMutation({
    mutationFn: async (project: ProjectDto) =>
      unwrapResult(
        await commands.removeProject({
          id: project.id,
          rowVersion: project.rowVersion,
        }),
      ),
    onSuccess: async (result, project) => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: projectKeys.all }),
        queryClient.invalidateQueries({ queryKey: mcpKeys.projects() }),
        queryClient.invalidateQueries({ queryKey: skillKeys.projects() }),
      ]);
      removeInFlightRef.current = false;
      setRemoveSubmitting(false);
      setRemoveDialogProject(null);
      notify({
        kind: "success",
        message: result.nativeConfigurationLeftUnmanaged
          ? `项目“${project.displayName}”已移除登记；项目目录和原生配置均未删除，已有原生配置已转为非受管。`
          : `项目“${project.displayName}”已移除登记；项目目录和原生配置均未删除。`,
      });
      if (isProjectRoute(pathname, project.id)) {
        void navigate("/projects");
      }
    },
    onError: (error, project) => {
      removeInFlightRef.current = false;
      setRemoveSubmitting(false);
      notify({
        kind: "error",
        message: `移除项目“${project.displayName}”失败：${
          profileErrorText(error) ?? "操作失败，请重新扫描后再试。"
        }`,
      });
    },
  });
  const removeBusy =
    removeDialogProject !== null ||
    removeMutation.isPending ||
    removeSubmitting;
  const renameBusy =
    renameDialogProject !== null ||
    renameMutation.isPending ||
    renameSubmitting;
  const renameSubmitDisabled =
    renameDisplayName.length === 0 ||
    renameDisplayName === renameDialogProject?.displayName;

  const requestRename = (project: ProjectDto) => {
    if (renameBusy || removeBusy || renameInFlightRef.current) return;
    renameMutation.reset();
    setRenameDisplayName(project.displayName);
    setRenameDialogProject(project);
  };

  const cancelRename = () => {
    if (
      renameMutation.isPending ||
      renameSubmitting ||
      renameInFlightRef.current
    ) {
      return;
    }
    renameMutation.reset();
    setRenameDialogProject(null);
    setRenameDisplayName("");
  };

  const submitRename = () => {
    if (
      !renameDialogProject ||
      renameSubmitDisabled ||
      renameMutation.isPending ||
      renameSubmitting ||
      renameInFlightRef.current
    ) {
      return;
    }
    renameInFlightRef.current = true;
    setRenameSubmitting(true);
    renameMutation.mutate();
  };

  const requestRemove = (project: ProjectDto) => {
    if (
      removeBusy ||
      renameBusy ||
      hasBlockedNativeResources(project.nativeResources) ||
      removeInFlightRef.current
    ) {
      return;
    }
    removeMutation.reset();
    setRemoveDialogProject(project);
  };

  const cancelRemove = () => {
    if (removeBusy && removeMutation.isPending) return;
    removeInFlightRef.current = false;
    setRemoveSubmitting(false);
    removeMutation.reset();
    setRemoveDialogProject(null);
  };

  const confirmRemove = () => {
    if (
      !removeDialogProject ||
      removeMutation.isPending ||
      removeSubmitting ||
      removeInFlightRef.current
    ) {
      return;
    }
    removeInFlightRef.current = true;
    setRemoveSubmitting(true);
    removeMutation.mutate(removeDialogProject);
  };

  return (
    <>
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
              className={cn(
                "size-3.5 transition-transform",
                open && "rotate-90",
              )}
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
              <p
                role="alert"
                className="px-2 py-1 text-xs text-red-700 dark:text-red-300"
              >
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
              <div key={project.id} className="group space-y-0.5">
                <div className="flex min-w-0 items-center gap-1">
                  <NavLink
                    to={`/projects/${project.id}`}
                    className={({ isActive }) =>
                      cn(
                        "min-w-0 flex-1 truncate rounded-md px-2 py-1.5 text-sm transition-colors",
                        isActive
                          ? "bg-muted text-foreground font-medium"
                          : "text-muted-foreground hover:bg-muted hover:text-foreground",
                      )
                    }
                    title={project.displayName}
                  >
                    {project.displayName}
                  </NavLink>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    className={projectRowActionClass}
                    aria-label={`编辑项目 ${project.displayName}`}
                    title={`编辑项目 ${project.displayName}`}
                    aria-haspopup="dialog"
                    disabled={renameBusy || removeBusy}
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      requestRename(project);
                    }}
                  >
                    <Pencil aria-hidden="true" className="size-3.5" />
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    className={projectRowActionClass}
                    aria-label={`移除项目 ${project.displayName}`}
                    title={`移除项目 ${project.displayName}`}
                    aria-describedby={
                      hasBlockedNativeResources(project.nativeResources)
                        ? `remove-project-blocked-${project.id}`
                        : undefined
                    }
                    disabled={
                      renameBusy ||
                      removeBusy ||
                      hasBlockedNativeResources(project.nativeResources)
                    }
                    onClick={(event) => {
                      event.preventDefault();
                      event.stopPropagation();
                      requestRemove(project);
                    }}
                  >
                    <Trash2 aria-hidden="true" className="size-3.5" />
                  </Button>
                </div>
                {hasBlockedNativeResources(project.nativeResources) ? (
                  <p
                    id={`remove-project-blocked-${project.id}`}
                    className="px-2 text-[11px] leading-4 text-amber-800 dark:text-amber-300"
                  >
                    无法移除：请先恢复已禁用或存在冲突的原生资源。
                  </p>
                ) : null}
              </div>
            ))}
          </div>
        ) : null}
      </div>
      <FormDialog
        open={renameDialogProject !== null}
        title="修改项目显示名称"
        description="仅修改应用内显示名称，不会修改项目目录或原生配置。"
        submitLabel="保存名称"
        pending={renameMutation.isPending || renameSubmitting}
        submitDisabled={renameSubmitDisabled}
        error={profileErrorText(renameMutation.error)}
        onClose={cancelRename}
        onSubmit={submitRename}
      >
        <div>
          <label
            htmlFor="project-display-name"
            className="mb-1 block text-sm font-medium"
          >
            显示名称
          </label>
          <input
            id="project-display-name"
            required
            maxLength={100}
            className="field"
            value={renameDisplayName}
            onChange={(event) =>
              setRenameDisplayName(event.currentTarget.value)
            }
          />
        </div>
      </FormDialog>
      <ProjectRemoveDialog
        project={removeDialogProject}
        pending={removeMutation.isPending}
        onClose={cancelRemove}
        onConfirm={confirmRemove}
      />
      <Notify notification={notification} />
    </>
  );
}

function hasBlockedNativeResources(project: ProjectDto["nativeResources"]) {
  return project.disabled + project.conflict > 0;
}

function isProjectRoute(pathname: string, projectId: string) {
  const projectPath = `/projects/${projectId}`;
  return pathname === projectPath || pathname.startsWith(`${projectPath}/`);
}

interface ProjectRemoveDialogProps {
  project: ProjectDto | null;
  pending: boolean;
  onClose: () => void;
  onConfirm: () => void;
}

function ProjectRemoveDialog({
  project,
  pending,
  onClose,
  onConfirm,
}: ProjectRemoveDialogProps) {
  const titleId = useId();
  const descriptionId = useId();
  const close = () => {
    if (!pending) onClose();
  };
  const { dialogRef, onKeyDown } = useDialogFocus(project !== null, close);

  if (!project) return null;

  return (
    <div
      role="presentation"
      className="fixed inset-0 z-50 grid place-items-center bg-slate-950/40 p-4"
    >
      <section
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className="bg-card w-full max-w-md rounded-xl p-6 shadow-xl"
      >
        <h2 id={titleId} className="text-xl font-semibold">
          确认移除项目
        </h2>
        <p
          id={descriptionId}
          className="text-muted-foreground mt-3 text-sm leading-6"
        >
          确定要移除项目“{project.displayName}
          ”的登记吗？此操作只移除登记，不删除项目目录或原生配置。
        </p>
        {pending ? (
          <p role="status" className="text-muted-foreground mt-4 text-sm">
            正在移除，请稍候…
          </p>
        ) : null}
        <div className="mt-6 flex justify-end gap-3">
          <Button
            type="button"
            variant="outline"
            disabled={pending}
            onClick={close}
          >
            取消
          </Button>
          <Button
            type="button"
            disabled={pending}
            onClick={() => {
              if (!pending) {
                dialogRef.current?.focus();
                onConfirm();
              }
            }}
          >
            {pending ? "正在移除…" : "确认移除"}
          </Button>
        </div>
      </section>
    </div>
  );
}
