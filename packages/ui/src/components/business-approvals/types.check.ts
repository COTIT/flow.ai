// Compile-time only (tsc): each hand-narrowed view type must stay assignable to the generated
// API model, so wire drift between the API and the UI fails the typecheck, not the screen.
import type {
  ApprovalRequestResource,
  CapabilityCheck,
  ConnectionResource,
  ExecutionResource,
  ExecutionSummary,
  MappingResource,
  PairResource,
} from "@verzo/api-client/model"

import type {
  ApprovalRequestView,
  CapabilityCheck as CapabilityView,
  ConnectionView,
  ExecutionView,
  MappingView,
  PairView,
} from "./types"

type Fits<View, Model> = View extends Model ? true : never
export const fits: [
  Fits<ApprovalRequestView, ApprovalRequestResource>,
  Fits<Omit<ExecutionView, "allowed_actions">, ExecutionSummary>,
  Fits<
    Required<ExecutionView>,
    Omit<
      ExecutionResource,
      | "approval_request_id"
      | "workflow_key"
      | "correlation_id"
      | "remote_attempts"
      | "manual_retry_grants"
      | "created_at"
    >
  >,
  Fits<ConnectionView, ConnectionResource>,
  Fits<CapabilityView, CapabilityCheck>,
  Fits<PairView, PairResource>,
  Fits<MappingView, MappingResource>,
] = [true, true, true, true, true, true, true]
