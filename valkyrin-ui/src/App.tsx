import { useCallback, useMemo, useEffect } from "react";
import {
  ReactFlow,
  Controls,
  Background,
  Panel,
  useNodesState,
  useEdgesState,
  addEdge,
} from "@xyflow/react";
import type { Connection, Edge, Node } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import TableNode from "./components/TableNode";

export default function App() {
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  const nodeTypes = useMemo(() => ({ table: TableNode }), []);

  // NEW: Fetch the saved blueprint from the Rust server on boot
  useEffect(() => {
    async function fetchBlueprint() {
      try {
        const response = await fetch("/api/load");
        const data = await response.json();

        if (data.tables && data.tables.length > 0) {
          // Restore the exact tables and their pixel coordinates
          const loadedNodes = data.tables.map((t: any) => ({
            id: t.id,
            type: "table",
            position: t.position, // Spatial tracking applied
            data: { label: t.name, columns: t.columns },
          }));
          setNodes(loadedNodes);

          // Restore the foreign key visual edges
          const loadedEdges = data.relations.map((r: any) => ({
            id: r.id,
            source: r.source_table_id,
            target: r.target_table_id,
            type: "default",
          }));
          setEdges(loadedEdges);
        }
      } catch (e) {
        console.error("No existing blueprint found on disk, starting fresh.");
      }
    }
    fetchBlueprint();
  }, [setNodes, setEdges]);

  const handleAddColumn = useCallback(
    (nodeId: string) => {
      const colName = window.prompt("Column Name (e.g., email, created_at):");
      if (!colName) return;

      const colType = window.prompt(
        "Data Type (string, int, boolean, datetime, uuid):",
        "string",
      );
      if (!colType) return;

      setNodes((nds) =>
        nds.map((node) => {
          if (node.id === nodeId) {
            const newCol = {
              id: crypto.randomUUID(),
              name: colName,
              raw_type: colType,
              is_primary: false,
              is_nullable: false,
            };
            return {
              ...node,
              data: {
                ...node.data,
                columns: [...(node.data.columns as any[]), newCol],
              },
            };
          }
          return node;
        }),
      );
    },
    [setNodes],
  );

  const nodesWithCallbacks = nodes.map((node) => ({
    ...node,
    data: { ...node.data, onAddColumn: handleAddColumn },
  }));

  const onConnect = useCallback(
    (params: Connection) => setEdges((eds) => addEdge(params, eds)),
    [setEdges],
  );

  const saveBlueprint = async () => {
    const payload = {
      tables: nodes.map((n) => ({
        id: n.id,
        name: n.data.label,
        columns: n.data.columns || [],
        position: n.position, // NEW: Save the exact X/Y layout
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
      if (response.ok) alert("Blueprint saved successfully!");
    } catch (error) {
      console.error("Network error:", error);
    }
  };

  return (
    <div
      style={{ width: "100vw", height: "100vh", backgroundColor: "#0f172a" }}
    >
      <ReactFlow
        nodes={nodesWithCallbacks}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        fitView
        colorMode="dark"
      >
        <Controls />
        <Background color="#334155" gap={24} />
        <Panel position="top-right">
          <button
            onClick={saveBlueprint}
            className="bg-blue-600 hover:bg-blue-500 text-white px-4 py-2 rounded-md font-bold shadow-lg"
          >
            Save Schema to Disk
          </button>
        </Panel>
      </ReactFlow>
    </div>
  );
}
