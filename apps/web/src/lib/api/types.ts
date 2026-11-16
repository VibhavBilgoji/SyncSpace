import type {
  CreateDocumentRequest,
  CreateWorkspaceRequest,
  Document,
  DocumentSummary,
  UpdateDocumentRequest,
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

export interface DocumentsApi {
  list(workspaceId: string): Promise<DocumentSummary[]>;
  create(workspaceId: string, input: CreateDocumentRequest): Promise<Document>;
  get(id: string): Promise<Document>;
  update(id: string, input: UpdateDocumentRequest): Promise<Document>;
  remove(id: string): Promise<void>;
}

export interface Api {
  auth: AuthApi;
  workspaces: WorkspacesApi;
  documents: DocumentsApi;
}
