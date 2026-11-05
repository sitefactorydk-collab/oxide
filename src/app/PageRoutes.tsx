import { Link, Navigate, useRoutes, type Location } from "react-router";
import { ArtifactsPage } from "@/features/artifacts/ArtifactsPage";
import { ChatPage } from "@/features/chat/ChatPage";
import { ChatsPage } from "@/features/chats/ChatsPage";
import { HomePage } from "@/features/home/HomePage";
import { ProjectPage } from "@/features/projects/ProjectPage";
import { ProjectsPage } from "@/features/projects/ProjectsPage";

/**
 * Page routes. `/settings/*` is not here: settings is a modal over whichever page was open,
 * so `AppLayout` renders these routes against that background location.
 */
export function PageRoutes({ location }: { location: string | Partial<Location> }) {
  return useRoutes(
    [
      { path: "/", element: <Navigate to="/new" replace /> },
      { path: "/new", element: <HomePage /> },
      { path: "/chat/:id", element: <ChatPage /> },
      { path: "/chats", element: <ChatsPage /> },
      { path: "/projects", element: <ProjectsPage /> },
      { path: "/project/:id", element: <ProjectPage /> },
      { path: "/artifacts", element: <ArtifactsPage /> },
      { path: "*", element: <NotFound /> },
    ],
    location,
  );
}

function NotFound() {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3">
      <p className="font-serif text-2xl text-fg">Nothing here</p>
      <Link to="/new" className="text-accent hover:underline">
        Go to a new chat
      </Link>
    </div>
  );
}
