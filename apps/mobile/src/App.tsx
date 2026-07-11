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
      const parsed = parsePairingLink(url);
      if (parsed) setLink(parsed);
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
