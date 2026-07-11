import type { PairingLink } from "@lol-overlay/protocol";
import { parsePairingLink } from "@lol-overlay/protocol";
import { StatusBar } from "expo-status-bar";
import { useEffect, useState } from "react";
import { Linking } from "react-native";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { GameScreen } from "./GameScreen";
import { PairingScreen } from "./PairingScreen";

export default function App() {
  const [link, setLink] = useState<PairingLink | null>(null);

  useEffect(() => {
    const consume = (url: string | null) => {
      if (!url) return;
      let parsed = parsePairingLink(url);
      if (!parsed) {
        try {
          const pair = new URLSearchParams(new URL(url).hash.slice(1)).get("pair");
          parsed = pair ? parsePairingLink(pair) : null;
        } catch {
          parsed = null;
        }
      }
      if (!parsed) return;
      setLink(parsed);
      if (typeof window !== "undefined" && window.location.hash) {
        window.history.replaceState(
          null,
          "",
          `${window.location.pathname}${window.location.search}`,
        );
      }
    };
    Linking.getInitialURL().then(consume);
    const subscription = Linking.addEventListener("url", ({ url }) => consume(url));
    return () => subscription.remove();
  }, []);

  return (
    <SafeAreaProvider>
      <StatusBar style="light" />
      {link ? (
        <GameScreen link={link} onDisconnect={() => setLink(null)} />
      ) : (
        <PairingScreen onPair={setLink} />
      )}
    </SafeAreaProvider>
  );
}
