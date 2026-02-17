import { createBrowserRouter, RouterProvider } from "react-router-dom";
import { App } from "./App";
import { StorybookPage } from "./pages/StorybookPage";

const router = createBrowserRouter([
  { path: "/", element: <App /> },
  { path: "/storybook", element: <StorybookPage /> },
]);

export function Router() {
  return <RouterProvider router={router} />;
}
