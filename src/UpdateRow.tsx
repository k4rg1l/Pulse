import { getVersion } from "@tauri-apps/api/app";
import { useEffect, useState } from "react";
import { checkForUpdate } from "./api";

/** Shows the installed version, and looks for a newer one on request. */
export function UpdateRow() {
  const [version, setVersion] = useState("");
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setVersion);
  }, []);

  function check() {
    setChecking(true);
    setResult(null);
    checkForUpdate()
      .then((available) =>
        setResult(available ? `Version ${available} is available` : "This is the latest version"),
      )
      .catch(() => setResult("Could not check for updates"))
      .finally(() => setChecking(false));
  }

  return (
    <li className="setting">
      <div className="setting-text">
        <span>Version {version}</span>
        <span className="caption" role="status">
          {result ?? "Pulse checks for updates every 6 hours."}
        </span>
      </div>
      <button className="button" disabled={checking} onClick={check}>
        {checking ? "Checking" : "Check for updates"}
      </button>
    </li>
  );
}
