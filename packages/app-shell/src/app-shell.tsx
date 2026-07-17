import { useState } from "react";
import { TooltipProvider } from "@ora/ui";
import { QueryClientProvider } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { ContractsClient } from "@ora/contracts";
import { ContractsClientContext } from "./contracts-client-context";
import { WorkspaceSidebar } from "./features/workspace/workspace-sidebar";
import { WorkspaceView } from "./features/workspace/workspace-view";
import { SettingsDialog } from "./features/settings/settings-dialog";
import { useSettingsPreferences } from "./features/settings/use-settings-preferences";
import { CONVERSATIONS_STORAGE_KEY, useConversations } from "./hooks/use-conversations";
import { useWorkspace } from "./hooks/use-workspace";
import { AppI18nProvider, type Locale } from "./i18n/i18n";
import { CURRENT_USER } from "./lib/mock-data";
import type { CurrentUser } from "./lib/types";
import { createAppQueryClient } from "./state/query-client";

interface AppShellProps {
  client: ContractsClient;
  user?: CurrentUser;
}

/** The main Ora application shell: sidebar + chat view with conversation state. */
export function AppShell({ client, user = CURRENT_USER }: AppShellProps) {
  // One client per shell instance so HMR or multiple mounted shells never share cache.
  const [queryClient] = useState(() => createAppQueryClient());
  return (
    <QueryClientProvider client={queryClient}>
      <AppI18nProvider>
        <AppShellContent client={client} user={user} />
      </AppI18nProvider>
    </QueryClientProvider>
  );
}

/** Renders the shell inside providers so stateful hooks can consume the active locale. */
function AppShellContent({ client, user }: Required<AppShellProps>) {
  const { i18n } = useTranslation();
  const locale: Locale = i18n.resolvedLanguage === "en-US" ? "en-US" : "zh-CN";
  const {
    activeConversation,
    clearConversations,
    isResponding,
    newChat,
    sendMessage,
  } = useConversations(locale);
  const workspace = useWorkspace(client);
  const { settings, updateSettings } = useSettingsPreferences();

  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);

  const handleSignOut = () => {
    try {
      window.localStorage.removeItem(CONVERSATIONS_STORAGE_KEY);
    } catch {
      /* Storage may be unavailable; reload anyway. */
    }
    window.location.reload();
  };

  return (
    <ContractsClientContext.Provider value={client}>
      <TooltipProvider>
        <div className="flex h-dvh overflow-hidden bg-background text-foreground">
          {!sidebarCollapsed && (
            <WorkspaceSidebar
              user={user}
              workspace={workspace}
              onCollapse={() => setSidebarCollapsed(true)}
              onOpenSettings={() => setSettingsOpen(true)}
              onSignOut={handleSignOut}
            />
          )}
          <WorkspaceView
            workspace={workspace}
            sidebarCollapsed={sidebarCollapsed}
            activeConversation={activeConversation}
            userName={user.name}
            isResponding={isResponding}
            onExpandSidebar={() => setSidebarCollapsed(false)}
            onSend={sendMessage}
            onNewChat={newChat}
          />
          <SettingsDialog
            open={settingsOpen}
            settings={settings}
            onOpenChange={setSettingsOpen}
            onUpdateSettings={updateSettings}
            onClearHistory={clearConversations}
          />
        </div>
      </TooltipProvider>
    </ContractsClientContext.Provider>
  );
}
