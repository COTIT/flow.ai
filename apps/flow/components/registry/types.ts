import type { FlowComponent } from "@genesy/api-client/model"

/** What every registry component gets: its saved config and whether the role may read it. */
export type RegistryProps = { component: FlowComponent; restricted: boolean }
