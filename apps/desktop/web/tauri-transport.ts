import { invoke } from "@tauri-apps/api/core";
import {
  ContractTransportError,
  type ContractTransport,
  type ContractTransportRequest,
  type EndpointOperation,
} from "@ora/contracts";

type TauriInvoke = <TResponse>(
  command: string,
  args: { request: unknown },
) => Promise<TResponse>;

const unsupportedOperations = {
  openProjectWorkContext: true,
  renewProjectWorkContext: true,
  listDirectory: true,
} as const satisfies Partial<Record<EndpointOperation, true>>;

type UnsupportedTauriOperation = keyof typeof unsupportedOperations;
type SupportedTauriOperation = Exclude<
  EndpointOperation,
  UnsupportedTauriOperation
>;

const tauriCommands = {
  createProject: "create_project",
  getProject: "get_project",
  listProjects: "list_projects",
  updateProject: "update_project",
  deleteProject: "delete_project",
  createTask: "create_task",
  getTask: "get_task",
  listTasks: "list_tasks",
  updateTask: "update_task",
  deleteTask: "delete_task",
  createSession: "create_session",
  getSession: "get_session",
  listSessions: "list_sessions",
  updateSession: "update_session",
  deleteSession: "delete_session",
  createSkill: "create_skill",
  getSkill: "get_skill",
  listSkills: "list_skills",
  updateSkill: "update_skill",
  deleteSkill: "delete_skill",
  createAgent: "create_agent",
  getAgent: "get_agent",
  listAgents: "list_agents",
  updateAgent: "update_agent",
  deleteAgent: "delete_agent",
} as const satisfies Record<SupportedTauriOperation, string>;

/** Creates the Desktop contracts transport backed by one typed Tauri command per operation. */
export function createTauriTransport(
  invokeCommand: TauriInvoke = invoke,
): ContractTransport {
  return {
    async send<TResponse>(request: ContractTransportRequest): Promise<TResponse> {
      const operation = request.operationName as EndpointOperation;

      if (operation in unsupportedOperations) {
        throw unsupportedOperation(operation);
      }

      const command = tauriCommands[operation as SupportedTauriOperation];
      if (!command) {
        throw unsupportedOperation(request.operationName);
      }

      try {
        return await invokeCommand<TResponse>(command, {
          request: request.request,
        });
      } catch (error) {
        if (error instanceof ContractTransportError) {
          throw error;
        }

        throw normalizeInvokeError(error);
      }
    },
  };
}

/** Builds the stable failure used for intentionally excluded Desktop operations. */
function unsupportedOperation(operationName: string): ContractTransportError {
  return new ContractTransportError({
    code: "unsupported_operation",
    message: `Desktop does not support operation ${operationName}`,
    status: null,
    responseBody: null,
  });
}

/** Normalizes serialized Rust command errors and opaque Tauri invocation failures. */
function normalizeInvokeError(error: unknown): ContractTransportError {
  if (isCommandError(error)) {
    return new ContractTransportError({
      code: error.code,
      message: error.message,
      status: null,
      responseBody: error,
    });
  }

  return new ContractTransportError({
    code: "tauri_invoke_error",
    message: "Desktop command invocation failed",
    status: null,
    responseBody: error,
  });
}

/** Narrows one rejected invocation value into the shared Rust command error shape. */
function isCommandError(error: unknown): error is { code: string; message: string } {
  if (typeof error !== "object" || error === null) {
    return false;
  }

  const record = error as Record<string, unknown>;
  return typeof record.code === "string" && typeof record.message === "string";
}
