import type {
  AddMemberRequest,
  AuditEntry,
  AuthProviders,
  Board,
  ChangeRoleRequest,
  CreateBoardRequest,
  CreateDocumentRequest,
  CreateTaskRequest,
  CreateWorkspaceRequest,
  Document,
  DocumentSummary,
  Member,
  MoveTaskRequest,
  Page,
  RestoreResponse,
  SaveVersionRequest,
  Task,
  UpdateDocumentRequest,
  UpdateTaskRequest,
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
  /** What the sign-in pages may offer (GitHub sign-in exists only when the server is set up). */
  providers(): Promise<AuthProviders>;
  /** A single-use ticket (30 s) for opening a WebSocket. */
  wsTicket(input: WsTicketRequest): Promise<WsTicketResponse>;
}

export interface WorkspacesApi {
  list(): Promise<WorkspaceSummary[]>;
  create(input: CreateWorkspaceRequest): Promise<Workspace>;
  /** The members of a workspace, the owner first. */
  members(workspaceId: string): Promise<Member[]>;
  addMember(workspaceId: string, input: AddMemberRequest): Promise<Member>;
  changeRole(workspaceId: string, userId: string, input: ChangeRoleRequest): Promise<Member>;
  removeMember(workspaceId: string, userId: string): Promise<void>;
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

export interface BoardsApi {
  list(workspaceId: string): Promise<Board[]>;
  create(workspaceId: string, input: CreateBoardRequest): Promise<Board>;
  /** The tasks of a board in column order, then position. */
  tasks(boardId: string): Promise<Task[]>;
  createTask(boardId: string, input: CreateTaskRequest): Promise<Task>;
  updateTask(id: string, input: UpdateTaskRequest): Promise<Task>;
  /** Moves a task to a column; without neighbours it goes to the end of that column. */
  moveTask(id: string, input: MoveTaskRequest): Promise<Task>;
  removeTask(id: string): Promise<void>;
}

export interface AuditApi {
  /** Newest first; ADMIN only. `action` narrows to one audited action. */
  list(
    workspaceId: string,
    options?: { limit?: number; cursor?: string; action?: string },
  ): Promise<Page<AuditEntry>>;
}

export interface Api {
  auth: AuthApi;
  workspaces: WorkspacesApi;
  documents: DocumentsApi;
  versions: VersionsApi;
  boards: BoardsApi;
  audit: AuditApi;
}
