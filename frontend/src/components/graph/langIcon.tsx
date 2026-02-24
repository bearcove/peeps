import React from "react";
import { FileCode } from "@phosphor-icons/react";
import RustOriginal from "devicons-react/lib/icons/RustOriginal";
import COriginal from "devicons-react/lib/icons/COriginal";
import CplusplusOriginal from "devicons-react/lib/icons/CplusplusOriginal";
import GoOriginal from "devicons-react/lib/icons/GoOriginal";
import PythonOriginal from "devicons-react/lib/icons/PythonOriginal";
import TypescriptOriginal from "devicons-react/lib/icons/TypescriptOriginal";
import JavascriptOriginal from "devicons-react/lib/icons/JavascriptOriginal";

type IconRenderer = (size: number, className?: string) => React.ReactNode;

const EXT_TO_ICON: Record<string, IconRenderer> = {
  rs: (size, cls) => <RustOriginal size={size} className={cls} />,
  c: (size, cls) => <COriginal size={size} className={cls} />,
  h: (size, cls) => <COriginal size={size} className={cls} />,
  cpp: (size, cls) => <CplusplusOriginal size={size} className={cls} />,
  cc: (size, cls) => <CplusplusOriginal size={size} className={cls} />,
  cxx: (size, cls) => <CplusplusOriginal size={size} className={cls} />,
  hpp: (size, cls) => <CplusplusOriginal size={size} className={cls} />,
  hh: (size, cls) => <CplusplusOriginal size={size} className={cls} />,
  go: (size, cls) => <GoOriginal size={size} className={cls} />,
  py: (size, cls) => <PythonOriginal size={size} className={cls} />,
  ts: (size, cls) => <TypescriptOriginal size={size} className={cls} />,
  tsx: (size, cls) => <TypescriptOriginal size={size} className={cls} />,
  js: (size, cls) => <JavascriptOriginal size={size} className={cls} />,
  jsx: (size, cls) => <JavascriptOriginal size={size} className={cls} />,
};

export function langIcon(sourceFile: string, size: number, className?: string): React.ReactNode {
  const ext = sourceFile.split(".").pop()?.toLowerCase() ?? "";
  const render = EXT_TO_ICON[ext];
  if (render) return render(size, className);
  return <FileCode size={size} className={className} />;
}
