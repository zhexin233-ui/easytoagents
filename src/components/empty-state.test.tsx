import { FolderOpen } from "lucide-react";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { EmptyState } from "@/components/empty-state";

describe("EmptyState", () => {
  it("渲染标题与说明", () => {
    render(<EmptyState title="尚无项目" description="登记第一个项目。" />);

    expect(screen.getByText("尚无项目")).toBeInTheDocument();
    expect(screen.getByText("登记第一个项目。")).toBeInTheDocument();
  });

  it("渲染自定义图标与下一步操作", () => {
    render(
      <EmptyState
        icon={FolderOpen}
        title="尚无项目"
        action={
          <button type="button" onClick={vi.fn()}>
            登记项目
          </button>
        }
      />,
    );

    expect(
      screen.getByRole("button", { name: "登记项目" }),
    ).toBeInTheDocument();
  });

  it("省略 description 与 action 时不渲染对应节点", () => {
    render(<EmptyState title="暂无数据" />);

    expect(screen.getByText("暂无数据")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});
