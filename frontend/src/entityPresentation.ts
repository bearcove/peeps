import type { EntityDef } from "./snapshot";

export function formatEntityPrimaryLabel(entity: Pick<EntityDef, "name">): string {
  return entity.name;
}

export function formatEntitySearchText(
  entity: Pick<
    EntityDef,
    "id" | "name" | "kind" | "source" | "processId" | "processName" | "processPid" | "krate" | "meta"
  >,
): string {
  return [
    entity.id,
    entity.name,
    entity.kind,
    entity.source,
    entity.processId,
    entity.processName,
    String(entity.processPid),
    entity.krate ?? "",
    JSON.stringify(entity.meta),
  ]
    .filter((part) => part.length > 0)
    .join(" ");
}
