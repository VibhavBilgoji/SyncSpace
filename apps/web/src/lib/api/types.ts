import type {
  CreateDocumentRequest,
  CreateWorkspaceRequest,
  Document,
  DocumentSummary,
  Page,
  RestoreResponse,
  SaveVersionRequest,
  UpdateDocumentRequest,
  VersionDetail,
  VersionSummary,
  WsTicketRequest,
  WsTicketResponse,
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
  /** A single-use ticket (30 s) for opening a WebSocket. */
  wsTicket(input: WsTicketRequest): Promise<WsTicketResponse>;
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

export interface VersionsApi {
  /** Newest first; `cursor` is the `next_cursor` of the previous page. */
  list(docId: string, options?: { limit?: number; cursor?: string }): Promise<Page<VersionSummary>>;
  create(docId: string, input: SaveVersionRequest): Promise<VersionSummary>;
  get(docId: string, number: number): Promise<VersionDetail>;
  restore(docId: string, number: number): Promise<RestoreResponse>;
}

export interface Api {
  auth: AuthApi;
  workspaces: WorkspacesApi;
  documents: DocumentsApi;
  versions: VersionsApi;
}
