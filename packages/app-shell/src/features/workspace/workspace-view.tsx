import { Button, Badge } from "@ora/ui";
import { useTranslation } from "react-i18next";
import {
  IconBrandGit,
  IconFolder,
  IconGitBranch,
  IconLayoutSidebarLeftExpand,
  IconPlayerPlay,
  IconRobot,
} from "@tabler/icons-react";
import type { WorkspaceData } from "../../hooks/use-workspace";
import { ChatView } from "../chat/chat-view";
import type { Conversation } from "../../lib/types";

interface WorkspaceViewProps {
  workspace: WorkspaceData;
  sidebarCollapsed: boolean;
  activeConversation: Conversation | null;
  userName: string;
  isResponding: boolean;
  onExpandSidebar: () => void;
  onSend: (text: string) => void;
  onNewChat: () => void;
}

/** Shows useful project/task context until a session is selected, then opens its agent chat. */
export function WorkspaceView({
  workspace,
  sidebarCollapsed,
  activeConversation,
  userName,
  isResponding,
  onExpandSidebar,
  onSend,
  onNewChat,
}: WorkspaceViewProps) {
  const { t } = useTranslation();
  const project = workspace.projects.find((item) => item.id === workspace.selection.projectId);
  const task = workspace.tasks.find((item) => item.id === workspace.selection.taskId);
  const session = workspace.sessions.find((item) => item.id === workspace.selection.sessionId);

  if (session && task && project) {
    return (
      <main className="relative flex min-w-0 flex-1 flex-col bg-background">
        <div className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
          {sidebarCollapsed && <Button variant="ghost" size="icon-sm" onClick={onExpandSidebar} aria-label={t("sidebar.expand")}><IconLayoutSidebarLeftExpand /></Button>}
          <IconRobot className="size-4 text-emerald-600" />
          <div className="min-w-0">
            <p className="truncate text-xs font-semibold">{task.title}</p>
            <p className="truncate text-[10px] text-muted-foreground">{project.name} / {session.agentId}</p>
          </div>
          <div className="flex-1" />
          <Badge variant="outline" className="gap-1 text-[10px]"><span className={`size-1.5 rounded-full ${session.status === "running" ? "bg-emerald-500" : "bg-zinc-400"}`} />{t(`common.${session.status}`)}</Badge>
        </div>
        <div className="min-h-0 flex-1 [&>main]:h-full">
          <ChatView active={activeConversation} userName={userName} isResponding={isResponding} onSend={onSend} onNewChat={onNewChat} />
        </div>
      </main>
    );
  }

  return (
    <main className="flex min-w-0 flex-1 flex-col bg-background">
      <header className="flex h-12 items-center border-b border-border px-3">
        {sidebarCollapsed && <Button variant="ghost" size="icon-sm" onClick={onExpandSidebar} aria-label={t("sidebar.expand")}><IconLayoutSidebarLeftExpand /></Button>}
        <span className="ml-1 text-xs font-medium text-muted-foreground">{t("workspace.overview")}</span>
      </header>
      <div className="flex flex-1 items-center justify-center p-6">
        <section className="w-full max-w-xl">
          <div className="mb-6 flex size-11 items-center justify-center rounded-lg border border-border bg-muted">
            {task ? <IconGitBranch className="size-5 text-sky-600" /> : <IconFolder className="size-5 text-amber-600" />}
          </div>
          <h1 className="text-xl font-semibold">{task?.title ?? project?.name ?? t("workspace.defaultTitle")}</h1>
          <p className="mt-2 max-w-md text-sm leading-6 text-muted-foreground">
            {task
              ? t("workspace.taskHint")
              : project
                ? t("workspace.projectHint")
                : t("workspace.emptyHint")}
          </p>
          {(project || task) && (
            <div className="mt-6 grid gap-px overflow-hidden rounded-md border border-border bg-border sm:grid-cols-2">
              <div className="bg-background p-4">
                <div className="flex items-center gap-2 text-xs text-muted-foreground"><IconBrandGit className="size-4" />{t("workspace.repository")}</div>
                <p className="mt-2 truncate text-sm font-medium">{project?.rootPath}</p>
              </div>
              <div className="bg-background p-4">
                <div className="flex items-center gap-2 text-xs text-muted-foreground"><IconPlayerPlay className="size-4" />{t("workspace.agentSessions")}</div>
                <p className="mt-2 text-sm font-medium">{task
                  ? t("workspace.sessionCount", { count: workspace.sessions.filter((item) => item.taskId === task.id).length })
                  : t("workspace.worktreeCount", { count: workspace.tasks.filter((item) => item.projectId === project?.id).length })}</p>
              </div>
            </div>
          )}
        </section>
      </div>
    </main>
  );
}
