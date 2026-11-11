import { SceneView } from "@/components/scenes/scene-view"

/** A full-screen scene (Morning Pulse, Stock Watch, …); the shell keeps the orb and the session. */
export default async function ScenePage({
  params,
}: {
  params: Promise<{ scene: string }>
}) {
  const { scene } = await params
  return <SceneView sceneKey={scene} />
}
