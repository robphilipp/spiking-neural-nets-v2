import {
  Activity,
  Code2,
  FolderOpen,
  History,
  PlayCircle,
  Radio,
  Server,
  Settings,
} from "lucide-react";
import { type ReactNode, useState } from "react";
import { Notice, StateBadge } from "@/components/kit";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { SessionPicker } from "./components/SessionPicker";
import { useConnection, useEngineError } from "./connection";
import { DeploymentPage } from "./pages/DeploymentPage";
import { InputsPage } from "./pages/InputsPage";
import { NetworkPage } from "./pages/NetworkPage";
import { ProjectsPage } from "./pages/ProjectsPage";
import { RecordingsPage } from "./pages/RecordingsPage";
import { RunPage } from "./pages/RunPage";
import { SettingsPage } from "./pages/SettingsPage";

type Page =
  "projects" | "network" | "run" | "deployment" | "inputs" | "recordings" | "settings";
const PAGES: Array<{ id: Page; label: string; icon: ReactNode }> = [
  { id: "projects", label: "Project", icon: <FolderOpen /> },
  { id: "network", label: "Network", icon: <Code2 /> },
  { id: "run", label: "Run", icon: <PlayCircle /> },
  { id: "deployment", label: "Deployment", icon: <Server /> },
  { id: "inputs", label: "Inputs", icon: <Radio /> },
  { id: "recordings", label: "Recordings", icon: <History /> },
  { id: "settings", label: "Settings", icon: <Settings /> },
];

export function App() {
  const [page, setPage] = useState<Page>("network");
  const { status } = useConnection();
  const engineError = useEngineError();
  return (
    <TooltipProvider>
      <SidebarProvider
        style={{ "--sidebar-width": "11rem" } as React.CSSProperties}
      >
        <Sidebar collapsible="icon">
          <SidebarHeader>
            <div className="flex h-8 items-center gap-2 px-2 font-semibold">
              <Activity className="size-4 shrink-0 text-series" />
              <span className="group-data-[collapsible=icon]:hidden">
                Spikes
              </span>
            </div>
          </SidebarHeader>
          <SidebarContent>
            <SidebarGroup>
              <SidebarGroupContent>
                <SidebarMenu>
                  {PAGES.map((p) => (
                    <SidebarMenuItem key={p.id}>
                      <SidebarMenuButton
                        isActive={page === p.id}
                        tooltip={p.label}
                        onClick={() => setPage(p.id)}
                        data-testid={`nav-${p.id}`}
                      >
                        {p.icon}
                        <span>{p.label}</span>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          </SidebarContent>
        </Sidebar>
        <SidebarInset className="min-w-0">
          <header className="sticky top-0 z-10 flex h-12 items-center gap-2 border-b bg-background/95 px-3 backdrop-blur">
            <SidebarTrigger />
            <SessionPicker />
            <div className="flex-1" />
            <StateBadge state={status} data-testid="connection" />
          </header>
          <div className="min-w-0 flex-1 p-4">
            {engineError.error && (
              <Notice
                tone="error"
                className="mb-4"
                onClose={engineError.dismiss}
                data-testid="engine-error"
              >
                The engine reported an error: {engineError.error.message}
                {engineError.error.count > 1 &&
                  ` (${engineError.error.count} times)`}
              </Notice>
            )}
            {page === "projects" && (
              <ProjectsPage onOpened={() => setPage("network")} />
            )}
            {page === "network" && <NetworkPage onRun={() => setPage("run")} />}
            {page === "run" && <RunPage onInputs={() => setPage("inputs")} />}
            {page === "deployment" && <DeploymentPage />}
            {page === "inputs" && <InputsPage />}
            {page === "recordings" && (
              <RecordingsPage onOpen={() => setPage("run")} />
            )}
            {page === "settings" && <SettingsPage />}
          </div>
        </SidebarInset>
      </SidebarProvider>
      <Toaster />
    </TooltipProvider>
  );
}
