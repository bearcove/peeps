import "./CratePill.css";

export function CratePill({ name }: { name: string }) {
  return (
    <span className="ui-crate-pill">
      {name}
    </span>
  );
}
