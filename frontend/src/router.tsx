import { createBrowserRouter, RouterProvider } from "react-router-dom";
import { App } from "./App";
import { StorybookPage, useStorybookState } from "./pages/StorybookPage";

function StorybookSplitScreen() {
  const state = useStorybookState();
  return (
    <div style={{ display: "flex" }}>
      <div style={{ flex: 1, minWidth: 0 }}>
        <StorybookPage colorScheme="dark" sharedState={state} />
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <StorybookPage colorScheme="light" sharedState={state} />
      </div>
    </div>
  );
}

const router = createBrowserRouter([
  { path: "/", element: <App /> },
  { path: "/storybook", element: <StorybookSplitScreen /> },
]);

export function Router() {
  return <RouterProvider router={router} />;
}
