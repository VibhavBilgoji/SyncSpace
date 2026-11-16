import type {
  CreateWorkspaceRequest,
  LoginRequest,
  RegisterRequest,
  User,
  Workspace,
  WorkspaceSummary,
} from "@/types/generated";

/** One interface per endpoint group; a real (http) and a mock implementation each. */
export interface AuthApi {
  register(input: RegisterRequest): Promise<User>;
  login(input: LoginRequest): Promise<User>;
  logout(): Promise<void>;
  me(): Promise<User>;
}

export interface WorkspacesApi {
  list(): Promise<WorkspaceSummary[]>;
  create(input: CreateWorkspaceRequest): Promise<Workspace>;
}

export interface Api {
  auth: AuthApi;
  workspaces: WorkspacesApi;
}
