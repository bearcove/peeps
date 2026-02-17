import { createBrowserRouter, RouterProvider } from "react-router-dom";
import { DeadlockDetectorPage } from "./pages/DeadlockDetectorPage";
import { LabPage } from "./pages/LabPage";

const router = createBrowserRouter([
  { path: "/", element: <DeadlockDetectorPage /> },
  { path: "/lab", element: <LabPage /> },
]);

export function Router() {
  return <RouterProvider router={router} />;
}
