import { fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";

import { PageHeader } from "@/components/page-header";

function renderHeader(props: Parameters<typeof PageHeader>[0]) {
  return render(
    <MemoryRouter>
      <PageHeader {...props} />
    </MemoryRouter>,
  );
}

describe("PageHeader", () => {
  it("渲染 h1 标题、右侧操作区与元信息", () => {
    renderHeader({
      title: "提示词",
      meta: <span>3 份档案</span>,
      actions: <button type="button">新建</button>,
    });

    expect(
      screen.getByRole("heading", { level: 1, name: "提示词" }),
    ).toBeInTheDocument();
    expect(screen.getByText("3 份档案")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "新建" })).toBeInTheDocument();
  });

  it("backTo 渲染返回链接并指向目标路由", () => {
    renderHeader({ title: "项目详情", backTo: "/projects" });

    const back = screen.getByRole("link", { name: "返回" });
    expect(back).toHaveAttribute("href", "/projects");
  });

  it("children 渲染在头部下方（tab / 过滤条）", () => {
    renderHeader({
      title: "Hooks",
      children: (
        <div>
          <button type="button" onClick={vi.fn()}>
            Claude
          </button>
        </div>
      ),
    });

    fireEvent.click(screen.getByRole("button", { name: "Claude" }));
    expect(screen.getByRole("button", { name: "Claude" })).toBeInTheDocument();
  });
});
