import { Suspense, useEffect, useId, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Bot,
  ChevronRight,
  FileText,
  FolderKanban,
  LayoutDashboard,
  Pencil,
  Plug,
  Settings,
  Sparkles,
  Trash2,
  Webhook,
} from "lucide-react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";

import { commands, type ProjectDto } from "@/bindings/commands";
import { FormDialog } from "@/components/form-dialog";
import { NotifyProvider } from "@/components/notify";
import { useDialogFocus } from "@/components/use-dialog-focus";
import { useEnabledTools } from "@/components/use-enabled-tools";
import { useTheme } from "@/components/use-theme";
import { useNotify } from "@/components/use-notify";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";
import { SettingsDialog } from "@/features/settings/settings-dialog";
import { useSubmitGuard } from "@/hooks/use-submit-guard";
import { invalidateEnvironmentDependents } from "@/lib/environment-api";
import { agentsKeys } from "@/lib/agents-api";
import { mcpKeys } from "@/lib/mcp-api";
import { profileErrorText, unwrapResult } from "@/lib/profile-api";
import { projectKeys, projectsQueryOptions } from "@/lib/projects-api";
import { skillKeys } from "@/lib/skills-api";
import { subscribeEnvironmentReady } from "@/lib/tauri-events";
import {
  PROFILE_TOOLS,
  filterEnabledTools,
  toolMetadata,
} from "@/lib/tool-metadata";
import { cn } from "@/lib/utils";

const primaryLinks = [
  { to: "/", label: "总览", end: true, icon: LayoutDashboard },
  { to: "/prompts", label: "提示词", end: false, icon: FileText },
  { to: "/mcp", label: "MCP", end: false, icon: Plug },
  { to: "/hooks", label: "Hooks", end: false, icon: Webhook },
  { to: "/skills", label: "Skills", end: false, icon: Sparkles },
  { to: "/agents", label: "Agents", end: false, icon: Bot },
] as const;

// NavLink 的 className 直接传 render-prop（不能包进 cn：clsx 会吞函数参数）。
const sidebarItemClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    "flex h-7 items-center gap-2 rounded-control px-2 text-[13px] transition-colors",
    isActive
      ? "bg-accent-soft font-medium text-accent"
      : "text-muted-foreground hover:bg-muted hover:text-foreground",
  );

const projectRowActionClass =
  "pointer-events-none opacity-0 transition-opacity group-hover:pointer-events-auto group-hover:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100 focus-visible:pointer-events-auto focus-visible:opacity-100";

export function AppShell() {
  const queryClient = useQueryClient();
  const [projectsExpanded, setProjectsExpanded] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { preference, setPreference } = useTheme();
  const { pathname } = useLocation();
  // 后端在后台探测工具环境；探测完成后所有依赖环境的查询重新拉取，
  // 启动期显示的"探测中"等待态随之变成真实状态。
  useEffect(
    () =>
      subscribeEnvironmentReady(() => {
        void invalidateEnvironmentDependents(queryClient);
      }),
    [queryClient],
  );
  const projectSectionOpen =
    projectsExpanded || pathname.startsWith("/projects/");

  return (
    <NotifyProvider>
      <div className="flex h-screen overflow-hidden">
        <aside className="flex w-[220px] shrink-0 flex-col border-r select-none">
          <nav
            aria-label="一级导航"
            className="min-h-0 flex-1 space-y-px overflow-y-auto px-2 pt-3"
          >
            {primaryLinks.map((link) => (
              <NavLink
                key={link.to}
                to={link.to}
                end={link.end}
                className={sidebarItemClass}
              >
                <link.icon aria-hidden="true" className="size-4 shrink-0" />
                {link.label}
              </NavLink>
            ))}
            <ProjectNavSection
              open={projectSectionOpen}
              onToggle={() => setProjectsExpanded((expanded) => !expanded)}
              onNavigate={() => setProjectsExpanded(true)}
            />
          </nav>
          <div className="px-2 pb-3">
            <button
              type="button"
              aria-haspopup="dialog"
              onClick={() => setSettingsOpen(true)}
              className={cn(sidebarItemClass({ isActive: false }))}
            >
              <Settings aria-hidden="true" className="size-4 shrink-0" />
              设置
            </button>
          </div>
        </aside>
        <div className="bg-background flex min-w-0 flex-1 flex-col">
          <TopBar />
          <div className="min-w-0 flex-1 overflow-y-auto">
            <Suspense fallback={<PageLoading />}>
              <Outlet />
            </Suspense>
          </div>
        </div>
        <SettingsDialog
          open={settingsOpen}
          onClose={() => setSettingsOpen(false)}
          themePreference={preference}
          onThemePreferenceChange={setPreference}
        />
      </div>
    </NotifyProvider>
  );
}

function PageLoading() {
  return (
    <p role="status" className="text-muted-foreground px-8 py-6">
      正在加载页面…
    </p>
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
    <header className="bg-background/80 flex h-11 shrink-0 items-center justify-end border-b px-4 backdrop-blur">
      <nav aria-label="工具入口" className="flex items-center gap-1">
        {toolLinks.map((link) => (
          <NavLink
            key={link.to}
            to={link.to}
            className={({ isActive }) =>
              cn(
                "flex h-7 items-center gap-1.5 rounded-full border border-transparent px-2.5 text-xs font-medium transition-colors",
                isActive
                  ? "bg-accent-soft text-accent"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )
            }
          >
            <img
              src={link.icon}
              alt=""
              aria-hidden="true"
              draggable={false}
              className="rounded-control size-4 object-contain"
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
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const projectsQuery = useQuery(projectsQueryOptions());
  const [renameDialogProject, setRenameDialogProject] =
    useState<ProjectDto | null>(null);
  const [renameDisplayName, setRenameDisplayName] = useState("");
  const [renameSubmitting, setRenameSubmitting] = useState(false);
  const renameGuard = useSubmitGuard();
  const [removeDialogProject, setRemoveDialogProject] =
    useState<ProjectDto | null>(null);
  const [removeSubmitting, setRemoveSubmitting] = useState(false);
  const removeGuard = useSubmitGuard();
  const { notify } = useNotify();
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
        queryClient.invalidateQueries({ queryKey: agentsKeys.projects() }),
      ]);
      renameGuard.end();
      setRenameSubmitting(false);
      setRenameDialogProject(null);
      setRenameDisplayName("");
      notify({
        kind: "success",
        message: `项目显示名称已修改为“${updated.displayName}”。`,
      });
    },
    onError: () => {
      renameGuard.end();
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
        queryClient.invalidateQueries({ queryKey: agentsKeys.projects() }),
      ]);
      removeGuard.end();
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
      removeGuard.end();
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
    if (renameBusy || removeBusy || renameGuard.isInFlight()) return;
    renameMutation.reset();
    setRenameDisplayName(project.displayName);
    setRenameDialogProject(project);
  };

  const cancelRename = () => {
    if (
      renameMutation.isPending ||
      renameSubmitting ||
      renameGuard.isInFlight()
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
      renameSubmitting
    ) {
      return;
    }
    if (!renameGuard.begin()) return;
    setRenameSubmitting(true);
    renameMutation.mutate();
  };

  const requestRemove = (project: ProjectDto) => {
    if (
      removeBusy ||
      renameBusy ||
      hasBlockedNativeResources(project.nativeResources) ||
      removeGuard.isInFlight()
    ) {
      return;
    }
    removeMutation.reset();
    setRemoveDialogProject(project);
  };

  const cancelRemove = () => {
    if (removeBusy && removeMutation.isPending) return;
    removeGuard.end();
    setRemoveSubmitting(false);
    removeMutation.reset();
    setRemoveDialogProject(null);
  };

  const confirmRemove = () => {
    if (!removeDialogProject || removeMutation.isPending || removeSubmitting) {
      return;
    }
    if (!removeGuard.begin()) return;
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
            className={sidebarItemClass}
            onClick={onNavigate}
          >
            <FolderKanban aria-hidden="true" className="size-4 shrink-0" />
            项目
          </NavLink>
          <button
            type="button"
            aria-expanded={open}
            aria-label={open ? "收起项目列表" : "展开项目列表"}
            title={open ? "收起项目列表" : "展开项目列表"}
            className="text-muted-foreground hover:bg-muted hover:text-foreground rounded-control flex size-6 shrink-0 items-center justify-center transition-colors"
            onClick={onToggle}
          >
            <ChevronRight
              aria-hidden="true"
              className={cn(
                "size-3.5 transition-transform",
                open && "rotate-90",
              )}
            />
          </button>
        </div>
        {open ? (
          <div className="space-y-px">
            {projectsQuery.isPending ? (
              <p
                role="status"
                className="text-muted-foreground px-2 py-1 text-xs"
              >
                正在读取项目…
              </p>
            ) : null}
            {projectsQuery.isError ? (
              <p role="alert" className="text-destructive px-2 py-1 text-xs">
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
                        "rounded-control flex h-7 min-w-0 flex-1 items-center pr-2 pl-6 text-[13px] transition-colors",
                        isActive
                          ? "bg-accent-soft text-accent font-medium"
                          : "text-muted-foreground hover:bg-muted hover:text-foreground",
                      )
                    }
                    title={project.displayName}
                  >
                    <span className="truncate">{project.displayName}</span>
                  </NavLink>
                  <Button
                    type="button"
                    size="icon"
                    variant="ghost"
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
                    size="icon"
                    variant="ghost"
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
                    className="text-warning px-2 pl-6 text-[11px] leading-4"
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
  const { dialogRef } = useDialogFocus(project !== null, close);

  if (!project) return null;

  return (
    <DialogOverlay>
      <DialogContent
        dialogRef={dialogRef}
        onClose={close}
        labelledBy={titleId}
        describedBy={descriptionId}
        size="sm"
      >
        <DialogHeader>
          <h2 id={titleId} className="text-[15px] font-semibold">
            确认移除项目
          </h2>
        </DialogHeader>
        <DialogBody>
          <p id={descriptionId} className="text-muted-foreground leading-6">
            确定要移除项目“{project.displayName}
            ”的登记吗？此操作只移除登记，不删除项目目录或原生配置。
          </p>
          {pending ? (
            <p role="status" className="text-muted-foreground mt-4">
              正在移除，请稍候…
            </p>
          ) : null}
        </DialogBody>
        <DialogFooter>
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
        </DialogFooter>
      </DialogContent>
    </DialogOverlay>
  );
}
