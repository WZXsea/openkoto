import React, { useEffect, useRef, useCallback, useId } from "react";
import { X } from "lucide-react";
import { cn } from "../../lib/utils";

interface DialogProps {
  /** @deprecated 使用 open 代替 */
  isOpen?: boolean;
  /** @deprecated 使用 onOpenChange 代替 */
  onClose?: () => void;
  /** 控制对话框是否打开 */
  open?: boolean;
  /** 对话框打开状态变化时的回调 */
  onOpenChange?: (open: boolean) => void;
  title?: string;
  children: React.ReactNode;
  className?: string;
  closeDisabled?: boolean;
}

import { createPortal } from "react-dom";

let nextDialogId = 0;
const openDialogIds = new Set<number>();

export function Dialog({
  isOpen,
  onClose,
  open,
  onOpenChange,
  title,
  children,
  className,
  closeDisabled = false,
}: DialogProps) {
  // 支持两种 API：旧的 isOpen/onClose 和新的 open/onOpenChange
  const isDialogOpen = open !== undefined ? open : isOpen;
  const handleClose = useCallback(() => {
    if (closeDisabled) return;
    if (onOpenChange) {
      onOpenChange(false);
    } else if (onClose) {
      onClose();
    }
  }, [closeDisabled, onOpenChange, onClose]);
  const dialogRef = useRef<HTMLDivElement>(null);
  const handleCloseRef = useRef(handleClose);
  const titleId = useId();
  const dialogId = useRef(++nextDialogId).current;

  useEffect(() => {
    handleCloseRef.current = handleClose;
  }, [handleClose]);

  useEffect(() => {
    const handleEscapeKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (dialogId !== Math.max(...openDialogIds)) return;
      handleCloseRef.current();
    };

    if (isDialogOpen) {
      openDialogIds.add(dialogId);
      const previouslyFocused = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      const previousBodyOverflow = document.body.style.overflow;
      document.addEventListener("keydown", handleEscapeKey);
      document.body.style.overflow = "hidden";
      dialogRef.current?.focus();
      return () => {
        openDialogIds.delete(dialogId);
        document.removeEventListener("keydown", handleEscapeKey);
        document.body.style.overflow = previousBodyOverflow;
        previouslyFocused?.focus();
      };
    }

    return () => {
      openDialogIds.delete(dialogId);
      document.removeEventListener("keydown", handleEscapeKey);
    };
  }, [dialogId, isDialogOpen]);

  if (!isDialogOpen) return null;

  return createPortal(
    // 外层容器：使用 fixed 定位覆盖整个视口，flex 实现垂直和水平居中
    <div className="fixed inset-0 z-[100] flex items-center justify-center overflow-y-auto">
      {/* Backdrop - 遮罩层，使用主题兼容的半透明背景 */}
      <div
        className="fixed inset-0 bg-background/80 backdrop-blur-sm"
        onClick={handleClose}
      />

      {/* Dialog - 弹窗主体，使用主题变量替代硬编码颜色 */}
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={title ? titleId : undefined}
        aria-label={title ? undefined : "Dialog"}
        tabIndex={-1}
        className={cn(
          // 使用 popover 主题变量，确保与当前主题匹配
          "relative z-[101] w-full max-w-lg rounded-xl bg-popover border border-border",
          "shadow-2xl p-6 mx-4 my-4",
          className
        )}
        onClick={(e) => e.stopPropagation()}
      >
        {title && (
          <div className="mb-4">
            {/* 标题使用主题前景色 */}
            <h2 id={titleId} className="text-xl font-semibold text-popover-foreground">{title}</h2>
          </div>
        )}
        {children}

        {/* Close button - 关闭按钮使用主题兼容的颜色 */}
        <button
          onClick={handleClose}
          disabled={closeDisabled}
          className="absolute top-4 right-4 text-muted-foreground hover:text-popover-foreground transition-colors"
          aria-label="Close"
        >
          <X size={20} />
        </button>
      </div>
    </div>,
    document.body
  );
}

interface DialogHeaderProps {
  children: React.ReactNode;
  className?: string;
}

export function DialogHeader({ children, className }: DialogHeaderProps) {
  return (
    <div className={cn("mb-4", className)}>
      {children}
    </div>
  );
}

interface DialogContentProps {
  children: React.ReactNode;
  className?: string;
}

export function DialogContent({ children, className }: DialogContentProps) {
  return <div className={cn("", className)}>{children}</div>;
}

interface DialogFooterProps {
  children: React.ReactNode;
  className?: string;
}

export function DialogFooter({ children, className }: DialogFooterProps) {
  return (
    <div className={cn("flex justify-end gap-3 mt-6", className)}>
      {children}
    </div>
  );
}

// DialogTitle 组件属性
interface DialogTitleProps {
  children: React.ReactNode;
  className?: string;
}

/**
 * DialogTitle 组件
 * 用于设置对话框标题
 */
export function DialogTitle({ children, className }: DialogTitleProps) {
  return (
    <h2 className={cn("text-lg font-semibold text-popover-foreground", className)}>
      {children}
    </h2>
  );
}
