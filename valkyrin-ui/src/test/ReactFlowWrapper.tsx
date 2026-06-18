import { ReactFlowProvider } from "@xyflow/react";
import { ReactNode } from "react";

interface ReactFlowWrapperProps {
  children: ReactNode;
}

export function ReactFlowWrapper({ children }: ReactFlowWrapperProps) {
  return <ReactFlowProvider>{children}</ReactFlowProvider>;
}