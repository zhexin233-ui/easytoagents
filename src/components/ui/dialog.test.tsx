import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import {
  DialogBody,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogOverlay,
} from "@/components/ui/dialog";

function renderDialog(onClose: () => void) {
  return render(
    <DialogOverlay>
      <DialogContent onClose={onClose} labelledBy="test-title">
        <DialogHeader>
          <h2 id="test-title">测试弹窗</h2>
        </DialogHeader>
        <DialogBody>
          <button type="button">弹窗内按钮</button>
        </DialogBody>
        <DialogFooter>
          <button type="button">取消</button>
        </DialogFooter>
      </DialogContent>
    </DialogOverlay>,
  );
}

describe("DialogContent outside click", () => {
  it("点击遮罩（弹窗外部）触发 onClose", () => {
    const onClose = vi.fn();
    renderDialog(onClose);

    fireEvent.mouseDown(screen.getByRole("presentation"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("点击弹窗内部不关闭", () => {
    const onClose = vi.fn();
    renderDialog(onClose);

    fireEvent.mouseDown(screen.getByRole("button", { name: "弹窗内按钮" }));
    fireEvent.mouseDown(screen.getByRole("dialog"));
    expect(onClose).not.toHaveBeenCalled();
  });

  it("未传 onClose 时不挂监听也不报错", () => {
    render(
      <DialogOverlay>
        <DialogContent labelledBy="test-title">
          <DialogBody>内容</DialogBody>
        </DialogContent>
      </DialogOverlay>,
    );

    fireEvent.mouseDown(screen.getByRole("presentation"));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});
