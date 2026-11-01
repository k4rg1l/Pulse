import { useEffect, useState } from "react";
import { getUpdate, installUpdate, onUpdateAvailable } from "./api";

/** Offers a new version of Pulse. Nothing is downloaded until the user asks. */
export function UpdateCard() {
  const [version, setVersion] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  useEffect(() => {
    getUpdate().then(setVersion);
    const available = onUpdateAvailable(setVersion);
    return () => {
      available.then((stop) => stop());
    };
  }, []);

  function install() {
    setInstalling(true);
    setFailure(null);
    installUpdate().catch((error) => {
      setInstalling(false);
      setFailure(String(error));
    });
  }

  if (!version) return null;

  return (
    <aside className="setup-card">
      <div>
        <h2>Pulse {version} is available</h2>
        <p className="caption" role={failure ? "alert" : undefined}>
          {failure ? `Could not update: ${failure}` : "Pulse restarts to install it."}
        </p>
      </div>
      <button className="button" disabled={installing} onClick={install}>
        {installing ? "Updating" : "Update"}
      </button>
    </aside>
  );
}
