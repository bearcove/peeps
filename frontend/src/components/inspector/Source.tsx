import React from "react";
import { FileRsIcon } from "@phosphor-icons/react";
import "./Source.css";

export function shortSource(source: string): string {
  const match = source.match(/^(.*):(\d+)$/);
  if (!match) {
    return source.split("/").pop() ?? source;
  }

  const [, path, line] = match;
  const file = path.split("/").pop() ?? path;
  return `${file}:${line}`;
}

export function Source({ source }: { source: string }) {
  return (
    <a
      className="inspector-source"
      href={`zed://file${source}`}
      title={`Open ${source} in Zed`}
    >
      <FileRsIcon size={11} weight="bold" />
      {shortSource(source)}
    </a>
  );
}
