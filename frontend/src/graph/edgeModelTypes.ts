// Edge routing type model (sketch) for compound ELK normalization.
// Intentionally separated from runtime code until wiring work starts.

type Brand<T, B extends string> = T & { readonly __brand: B };

// ── Branded IDs ───────────────────────────────────────────────

export type GraphId = Brand<string, "GraphId">;
export type NodeId = Brand<string, "NodeId">;
export type EdgeId = Brand<string, "EdgeId">;
export type SectionId = Brand<string, "SectionId">;

// Optional extra layer for post-normalization/render identifiers.
export type RenderedEdgeId = Brand<string, "RenderedEdgeId">;
export type RenderedSectionId = Brand<string, "RenderedSectionId">;

// ── Branded coordinates ───────────────────────────────────────

export type LocalPoint = Brand<Readonly<{ x: number; y: number }>, "LocalPoint">;
export type WorldPoint = Brand<Readonly<{ x: number; y: number }>, "WorldPoint">;
export type GraphOffset = Brand<Readonly<{ dx: number; dy: number }>, "GraphOffset">;

export type LocalPolyline = ReadonlyArray<LocalPoint>;
export type WorldPolyline = ReadonlyArray<WorldPoint>;

// ── Shared linkage ────────────────────────────────────────────

export type SectionLinks = Readonly<{
  incoming: ReadonlyArray<SectionId>;
  outgoing: ReadonlyArray<SectionId>;
}>;

// ── Hierarchy transforms ──────────────────────────────────────

export type GraphTransformNode = Readonly<{
  graphId: GraphId;
  parentGraphId: GraphId | null;
  offsetToParent: GraphOffset;
}>;

export type GraphTransformIndex = ReadonlyMap<GraphId, GraphTransformNode>;

// ── Raw ELK fragments (hierarchical/local) ────────────────────

export type RawSectionFragment = Readonly<{
  edge: EdgeId;
  section: SectionId | null;
  owner: GraphId;
  links: SectionLinks;
  points: LocalPolyline;
}>;

export type RawFragmentsByEdge = ReadonlyMap<EdgeId, ReadonlyArray<RawSectionFragment>>;

// ── Normalized world-space fragments ──────────────────────────

export type WorldSectionFragment = Readonly<{
  edge: EdgeId;
  section: SectionId | null;
  owner: GraphId;
  links: SectionLinks;
  points: WorldPolyline;
}>;

export type WorldFragmentsByEdge = ReadonlyMap<EdgeId, ReadonlyArray<WorldSectionFragment>>;

// ── Chaining output ───────────────────────────────────────────

export type SectionChain = Readonly<{
  edge: EdgeId;
  sections: ReadonlyArray<SectionId>;
  points: WorldPolyline;
}>;

// ── Final rendered route ──────────────────────────────────────

export type RenderedEdgeRoute = Readonly<{
  rendered: RenderedEdgeId;
  source: NodeId;
  target: NodeId;
  points: WorldPolyline;
}>;

// ── Helpers to construct branded values at boundaries ─────────

export const asGraphId = (value: string): GraphId => value as GraphId;
export const asNodeId = (value: string): NodeId => value as NodeId;
export const asEdgeId = (value: string): EdgeId => value as EdgeId;
export const asSectionId = (value: string): SectionId => value as SectionId;
export const asRenderedEdgeId = (value: string): RenderedEdgeId => value as RenderedEdgeId;
export const asRenderedSectionId = (value: string): RenderedSectionId => value as RenderedSectionId;

export const asLocalPoint = (x: number, y: number): LocalPoint =>
  ({ x, y }) as LocalPoint;
export const asWorldPoint = (x: number, y: number): WorldPoint =>
  ({ x, y }) as WorldPoint;
export const asGraphOffset = (dx: number, dy: number): GraphOffset =>
  ({ dx, dy }) as GraphOffset;
