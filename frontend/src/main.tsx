import { createRoot } from "react-dom/client";
import "./styles.css";
import { Router } from "./router";

createRoot(document.getElementById("app")!).render(<Router />);
