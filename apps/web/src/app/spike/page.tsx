import SpikeEditor from "./SpikeEditor";

// THROWAWAY (roadmap task 0.3): proves the Wasm CRDT works in the browser.
export default function SpikePage() {
  return (
    <main>
      <h1>CRDT spike</h1>
      <p>Open this page in two tabs and type: both converge.</p>
      <SpikeEditor />
    </main>
  );
}
