import { LogOut, RefreshCw, User } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { BackendUser } from "../../lib/tauri";
import { Button } from "../ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "../ui/dropdown-menu";

interface AccountMenuProps {
  user: BackendUser | null | undefined;
  backendUrl?: string | null;
  isLoggingOut?: boolean;
  onLogout: () => void | Promise<void>;
  onSwitchAccount: () => void | Promise<void>;
}

function accountInitial(user: BackendUser | null | undefined): string {
  const source = user?.display_name?.trim() || user?.email?.trim() || "?";
  return source.slice(0, 1).toUpperCase();
}

function accountLabel(user: BackendUser | null | undefined, fallback: string): string {
  return user?.display_name?.trim() || user?.email?.trim() || fallback;
}

export function AccountMenu({
  user,
  backendUrl,
  isLoggingOut = false,
  onLogout,
  onSwitchAccount,
}: AccountMenuProps) {
  const { t } = useTranslation();
  const label = accountLabel(user, t("account.signedIn", "已登录"));

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="max-w-[220px] gap-2 px-2"
          aria-label={t("account.menuLabel", "账户")}
        >
          <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-primary/10 text-xs font-semibold text-primary">
            {accountInitial(user)}
          </span>
          <span className="min-w-0 truncate">{label}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-72">
        <DropdownMenuLabel>
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-sm">
              <User size={15} className="shrink-0 text-muted-foreground" />
              <span className="truncate">{label}</span>
            </div>
            {user?.email && (
              <div className="mt-1 truncate text-xs font-normal text-muted-foreground">
                {user.email}
              </div>
            )}
            {backendUrl && (
              <div className="mt-1 truncate text-xs font-normal text-muted-foreground">
                {backendUrl}
              </div>
            )}
          </div>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuItem
          onSelect={(event) => {
            event.preventDefault();
            void onSwitchAccount();
          }}
          disabled={isLoggingOut}
        >
          <RefreshCw size={15} />
          {t("account.switch", "切换账户")}
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={(event) => {
            event.preventDefault();
            void onLogout();
          }}
          disabled={isLoggingOut}
          className="text-destructive focus:text-destructive"
        >
          <LogOut size={15} />
          {isLoggingOut ? t("account.loggingOut", "正在退出") : t("account.logout", "退出登录")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
