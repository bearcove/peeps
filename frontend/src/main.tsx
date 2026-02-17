import { createRoot } from "react-dom/client";
import "@xyflow/react/dist/style.css";
import { App } from "./App";
import "./styles.css";

createRoot(document.getElementById("app")!).render(<App />);
