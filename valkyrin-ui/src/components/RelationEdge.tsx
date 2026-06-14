import {
  BaseEdge,
  EdgeLabelRenderer,
  getBezierPath,
} from "@xyflow/react";
import type { EdgeProps, } from "@xyflow/react";
import type { RelationType } from "../types";
import { useState } from "react";

interface RelationEdgeProps extends EdgeProps {
  data?: {
    relationshipType?: RelationType;
    onRelationshipChange?: (newType: RelationType) => void;
  };
}

/**
 * Custom Edge component with interactive relation badge
 * Displays and allows cycling through 1:1, 1:N, M:N relationship types
 */
export default function RelationEdge({
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  data,
  style,
}: RelationEdgeProps) {
  const [edgePath, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });

  const [isHovering, setIsHovering] = useState(false);

  // Default relation type
  const relationshipType: RelationType = data?.relationshipType || "1:N";

  const cycleRelationshipType = () => {
    const types: RelationType[] = ["1:1", "1:N", "M:N"];
    const currentIndex = types.indexOf(relationshipType);
    const nextType = types[(currentIndex + 1) % types.length];

    if (data?.onRelationshipChange) {
      data.onRelationshipChange(nextType);
    }
  };

  return (
    <>
      {/* Bezier curve edge */}
      <BaseEdge path={edgePath} style={style} />

      {/* Edge Label Renderer with Interactive Badge */}
      <EdgeLabelRenderer>
        <div
          style={{
            position: "absolute",
            transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
            pointerEvents: "all",
          }}
          className="z-10"
          onMouseEnter={() => setIsHovering(true)}
          onMouseLeave={() => setIsHovering(false)}
        >
          {/* Relation Type Badge */}
          <button
            onClick={cycleRelationshipType}
            className={`px-3 py-1.5 rounded-full font-bold text-xs transition-all duration-200 cursor-pointer shadow-lg ${
              isHovering
                ? "bg-cyan-500 text-white scale-110 shadow-cyan-500/50"
                : "bg-zinc-800 text-cyan-300 border border-cyan-600/50 hover:border-cyan-400"
            }`}
            title={`Click to cycle through relation types (current: ${relationshipType})`}
          >
            {relationshipType}
          </button>

          {/* Hover Helper Text */}
          {isHovering && (
            <div className="absolute -top-8 left-1/2 -translate-x-1/2 bg-zinc-900 text-white text-[10px] px-2 py-1 rounded border border-zinc-700 whitespace-nowrap">
              Click to cycle 1:1 → 1:N → M:N
            </div>
          )}
        </div>
      </EdgeLabelRenderer>
    </>
  );
}
