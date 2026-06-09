import { useCallback } from "react";
import {
  ReactFlow,
  Controls,
  Background,
  Panel,
  useNodesState,
  useEdgesState,
  addEdge,
} from "@xyflow/react";
import type { Connection, Edge } from "@xyflow/react";
import "@xyflow/react/dist/style.css";

const initialNodes = [
  { id: "1", position: { x: 100, y: 100 }, data: { label: "Users Table" } },
  { id: "2", position: { x: 400, y: 100 }, data: { label: "Sessions Table" } },
];

const initialEdges: Edge[] = [];

export default function App() {
  const [nodes, , onNodesChange] = useNodesState(initialNodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState(initialEdges);

  const onConnect = useCallback(
    (params: Connection) => setEdges((eds) => addEdge(params, eds)),
    [setEdges],
  );

  // NEW: The Bridge Function
  const saveBlueprint = async () => {
    // Construct the payload matching our Rust CanvasPayload schema
    const payload = {
      tables: nodes.map((n) => ({
        id: n.id,
        name: n.data.label,
        columns: [], // We will add column modeling later
      })),
      relations: edges.map((e) => ({
        id: e.id,
        source_table_id: e.source,
        target_table_id: e.target,
        relation_type: "1:N",
      })),
    };

    try {
      const response = await fetch("/api/save", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });

      if (response.ok) {
        alert("Blueprint saved successfully!");
      } else {
        alert("Failed to save blueprint.");
      }
    } catch (error) {
      console.error("Network error:", error);
    }
  };

  return (
    <div style={{ width: "100vw", height: "100vh" }}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        fitView
      >
        <Controls />
        <Background />

        {/* NEW: The Floating Save Button */}
        <Panel position="top-right">
          <button
            onClick={saveBlueprint}
            style={{
              backgroundColor: "#3b82f6",
              color: "white",
              padding: "8px 16px",
              borderRadius: "6px",
              border: "none",
              fontWeight: "bold",
              cursor: "pointer",
              boxShadow: "0 4px 6px -1px rgb(0 0 0 / 0.1)",
            }}
          >
            Save Schema to Disk
          </button>
        </Panel>
      </ReactFlow>
    </div>
  );
}
