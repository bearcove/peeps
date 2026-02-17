import { useEffect, useMemo, useRef } from "react";
import { LabView } from "./components/LabView";

export function LabApp() {
  const hydratedRef = useRef(false);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    params.set("mode", "lab");
    params.delete("lab_tone");
    const nextSearch = params.toString();
    const nextUrl = `${window.location.pathname}${nextSearch ? `?${nextSearch}` : ""}${window.location.hash}`;
    const currentUrl = `${window.location.pathname}${window.location.search}${window.location.hash}`;
    if (currentUrl === nextUrl) {
      hydratedRef.current = true;
      return;
    }
    if (hydratedRef.current) window.history.pushState(null, "", nextUrl);
    else {
      window.history.replaceState(null, "", nextUrl);
      hydratedRef.current = true;
    }
  }, []);

  const title = useMemo(() => "Peeps Lab", []);

  return (
    <div className="lab-app" aria-label={title}>
      <LabView />
    </div>
  );
}
