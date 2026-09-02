import { useCallback, useEffect, useRef, useState } from "react";

import { Toast } from "./components/Toast";
import { ProjectsScreen } from "./screens/ProjectsScreen";
import { Workspace } from "./screens/Workspace";
import type { Project } from "./types";

import "./styles.css";

/**
 * Two screens: pick a project, then work inside it. The toast lives up here
 * because both screens raise them and only one may be on screen at a time.
 */
export default function App() {
  const [openProject, setOpenProject] = useState<Project | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout>>(undefined);

  const say = useCallback((message: string) => {
    setToast(message);
    clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 2600);
  }, []);

  useEffect(() => () => clearTimeout(toastTimer.current), []);

  return (
    <div className="screen">
      {openProject ? (
        <Workspace project={openProject} say={say} onLeave={() => setOpenProject(null)} />
      ) : (
        <ProjectsScreen say={say} onOpen={setOpenProject} />
      )}
      <Toast message={toast} />
    </div>
  );
}
