/* eslint-disable @typescript-eslint/unbound-method -- 生成 command 是无 this 的函数集合，测试直接核验 mock。 */
import { useState } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { commands } from "@/bindings/commands";
import { SnapshotRestoreDialog } from "@/components/snapshot-restore-dialog";

vi.mock("@/bindings/commands", () => ({
  commands: {
    listSnapshots: vi.fn(),
    deleteSnapshots: vi.fn(),
    previewSnapshotRestore: vi.fn(),
    restoreSnapshot: vi.fn(),
  },
}));

function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button onClick={() => setOpen(true)}>打开恢复</button>
      <SnapshotRestoreDialog open={open} onClose={() => setOpen(false)} />
    </>
  );
}

function renderHarness() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>,
  );
}

describe("SnapshotRestoreDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(commands.listSnapshots).mockResolvedValue({
      status: "ok",
      data: [
        {
          snapshotId: "00000000-0000-4000-8000-000000000731",
          runId: "00000000-0000-4000-8000-000000000732",
          targetId: "00000000-0000-4000-8000-000000000733",
          targetPath: "/isolated/home/.codex/config.toml",
          targetType: "file",
          createdAt: "2026-08-24T10:00:00Z",
        },
      ],
    });
    vi.mocked(commands.previewSnapshotRestore).mockResolvedValue({
      status: "ok",
      data: {
        previewId: "00000000-0000-4000-8000-000000000734",
        snapshotId: "00000000-0000-4000-8000-000000000731",
        targetPath: "/isolated/home/.codex/config.toml",
        currentType: "file",
        snapshotType: "file",
      },
    });
    vi.mocked(commands.restoreSnapshot).mockResolvedValue({
      status: "ok",
      data: {
        runId: "restore-1",
        status: "succeeded",
        appliedTargets: 1,
        snapshotCount: 1,
      },
    });
    vi.mocked(commands.deleteSnapshots).mockResolvedValue({
      status: "ok",
      data: { deletedIds: [], failures: [] },
    });
  });

  afterEach(cleanup);

  it("通过持久化恢复预览执行恢复，并在关闭后恢复触发按钮焦点", async () => {
    renderHarness();
    const trigger = screen.getByRole("button", { name: "打开恢复" });
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = await screen.findByRole("dialog", {
      name: "恢复原生目标快照",
    });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(screen.getByRole("button", { name: "关闭" })).toHaveFocus();

    fireEvent.click(await screen.findByRole("button", { name: "预览恢复" }));
    expect(await screen.findByText("确认恢复此目标")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "执行恢复" }));

    await waitFor(() =>
      expect(commands.restoreSnapshot).toHaveBeenCalledWith({
        previewId: "00000000-0000-4000-8000-000000000734",
        snapshotId: "00000000-0000-4000-8000-000000000731",
      }),
    );
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it("Escape 关闭时清除旧预览并在重开后恢复列表与焦点", async () => {
    renderHarness();
    const trigger = screen.getByRole("button", { name: "打开恢复" });
    trigger.focus();
    fireEvent.click(trigger);
    const dialog = await screen.findByRole("dialog", {
      name: "恢复原生目标快照",
    });
    fireEvent.click(await screen.findByRole("button", { name: "预览恢复" }));
    expect(await screen.findByText("确认恢复此目标")).toBeInTheDocument();
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(trigger).toHaveFocus());

    fireEvent.click(trigger);
    expect(
      await screen.findByRole("button", { name: "预览恢复" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("确认恢复此目标")).not.toBeInTheDocument();
  });

  it("勾选多个恢复点后经二次确认执行删除选中，并清空选择", async () => {
    vi.mocked(commands.listSnapshots).mockResolvedValue({
      status: "ok",
      data: [
        {
          snapshotId: "00000000-0000-4000-8000-000000000731",
          runId: "00000000-0000-4000-8000-000000000732",
          targetId: "00000000-0000-4000-8000-000000000733",
          targetPath: "/isolated/home/.codex/config.toml",
          targetType: "file",
          createdAt: "2026-08-24T10:00:00Z",
        },
        {
          snapshotId: "00000000-0000-4000-8000-000000000741",
          runId: "00000000-0000-4000-8000-000000000742",
          targetId: "00000000-0000-4000-8000-000000000743",
          targetPath: "/isolated/home/.claude/CLAUDE.md",
          targetType: "file",
          createdAt: "2026-08-24T11:00:00Z",
        },
      ],
    });
    vi.mocked(commands.deleteSnapshots).mockResolvedValue({
      status: "ok",
      data: {
        deletedIds: [
          "00000000-0000-4000-8000-000000000731",
          "00000000-0000-4000-8000-000000000741",
        ],
        failures: [],
      },
    });
    renderHarness();
    fireEvent.click(await screen.findByRole("button", { name: "打开恢复" }));
    fireEvent.click(
      await screen.findByRole("checkbox", {
        name: "选择 /isolated/home/.codex/config.toml",
      }),
    );
    fireEvent.click(
      screen.getByRole("checkbox", {
        name: "选择 /isolated/home/.claude/CLAUDE.md",
      }),
    );
    expect(commands.deleteSnapshots).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "删除选中 (2)" }));
    expect(
      await screen.findByText(/将永久删除 2 个恢复点/),
    ).toBeInTheDocument();
    expect(commands.deleteSnapshots).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));

    await waitFor(() =>
      expect(commands.deleteSnapshots).toHaveBeenCalledWith({
        snapshotIds: [
          "00000000-0000-4000-8000-000000000731",
          "00000000-0000-4000-8000-000000000741",
        ],
      }),
    );
    expect(await screen.findByText("已删除 2 个恢复点。")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByRole("checkbox", {
          name: "选择 /isolated/home/.codex/config.toml",
        }),
      ).not.toBeChecked(),
    );
    expect(
      screen.getByRole("checkbox", {
        name: "选择 /isolated/home/.claude/CLAUDE.md",
      }),
    ).not.toBeChecked();
  });

  it("全部删除经二次确认后传入列表全量 ID", async () => {
    vi.mocked(commands.deleteSnapshots).mockResolvedValue({
      status: "ok",
      data: {
        deletedIds: ["00000000-0000-4000-8000-000000000731"],
        failures: [],
      },
    });
    renderHarness();
    fireEvent.click(await screen.findByRole("button", { name: "打开恢复" }));
    fireEvent.click(await screen.findByRole("button", { name: "全部删除" }));
    expect(
      await screen.findByText(/将永久删除 1 个恢复点/),
    ).toBeInTheDocument();
    expect(commands.deleteSnapshots).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));

    await waitFor(() =>
      expect(commands.deleteSnapshots).toHaveBeenCalledWith({
        snapshotIds: ["00000000-0000-4000-8000-000000000731"],
      }),
    );
    expect(await screen.findByText("已删除 1 个恢复点。")).toBeInTheDocument();
  });

  it("删除确认可取消，未确认不发起删除调用", async () => {
    renderHarness();
    fireEvent.click(await screen.findByRole("button", { name: "打开恢复" }));
    fireEvent.click(
      await screen.findByRole("checkbox", {
        name: "选择 /isolated/home/.codex/config.toml",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "删除选中 (1)" }));
    expect(
      await screen.findByText(/将永久删除 1 个恢复点/),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    expect(screen.queryByText(/将永久删除/)).not.toBeInTheDocument();
    expect(commands.deleteSnapshots).not.toHaveBeenCalled();
  });

  it("删除结果含失败项时展示成功与失败计数", async () => {
    vi.mocked(commands.deleteSnapshots).mockResolvedValue({
      status: "ok",
      data: {
        deletedIds: ["00000000-0000-4000-8000-000000000731"],
        failures: [
          {
            snapshotId: "00000000-0000-4000-8000-000000000999",
            code: "CONFLICT",
            message: "快照仍被活动 run 引用，需等待恢复完成后删除",
          },
        ],
      },
    });
    renderHarness();
    fireEvent.click(await screen.findByRole("button", { name: "打开恢复" }));
    fireEvent.click(await screen.findByRole("button", { name: "全部删除" }));
    fireEvent.click(await screen.findByRole("button", { name: "确认删除" }));

    expect(
      await screen.findByText("已删除 1 个恢复点，1 个删除失败。"),
    ).toBeInTheDocument();
  });
});
