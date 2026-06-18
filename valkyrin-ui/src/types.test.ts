import { describe, it, expect } from "vitest";
import {
  isValidIdentifier,
  validateTableName,
  validateColumnName,
  validateDecimal,
  validateEnumValues,
  Validation,
} from "./types";

describe("Validation Utilities", () => {
  describe("isValidIdentifier", () => {
    it("returns true for valid identifiers", () => {
      expect(isValidIdentifier("users")).toBe(true);
      expect(isValidIdentifier("_users")).toBe(true);
      expect(isValidIdentifier("Users")).toBe(true);
      expect(isValidIdentifier("user123")).toBe(true);
      expect(isValidIdentifier("user_name")).toBe(true);
      expect(isValidIdentifier("a")).toBe(true);
    });

    it("returns false for invalid identifiers", () => {
      expect(isValidIdentifier("")).toBe(false);
      expect(isValidIdentifier("123users")).toBe(false);
      expect(isValidIdentifier("user-name")).toBe(false);
      expect(isValidIdentifier("user.name")).toBe(false);
      expect(isValidIdentifier("user name")).toBe(false);
      expect(isValidIdentifier("user@name")).toBe(false);
      expect(isValidIdentifier("-users")).toBe(false);
    });
  });

  describe("validateTableName", () => {
    it("returns valid for unique, valid table name", () => {
      const result = validateTableName("users", ["posts", "comments"]);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it("returns error for empty name", () => {
      const result = validateTableName("", ["posts"]);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "name",
        message: "Table name is required",
      });
    });

    it("returns error for invalid identifier format", () => {
      const result = validateTableName("123users", ["posts"]);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "name",
        message: "Table name must start with letter or underscore, followed by alphanumerics or underscores only",
      });
    });

    it("returns error for duplicate name", () => {
      const result = validateTableName("users", ["users", "posts"]);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "name",
        message: "A table with this name already exists",
      });
    });

    it("ignores excluded table ID when checking duplicates", () => {
      const result = validateTableName("users", ["users", "posts"], "users");
      expect(result.valid).toBe(true);
    });

    it("ignores excluded table ID when name matches excluded", () => {
      const result = validateTableName("users", ["users", "posts"], "other-id");
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "name",
        message: "A table with this name already exists",
      });
    });
  });

  describe("validateColumnName", () => {
    it("returns valid for unique, valid column name", () => {
      const result = validateColumnName("email", ["id", "name"]);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it("returns error for empty name", () => {
      const result = validateColumnName("", ["id"]);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "name",
        message: "Column name is required",
      });
    });

    it("returns error for invalid identifier format", () => {
      const result = validateColumnName("123email", ["id"]);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "name",
        message: "Column name must start with letter or underscore, followed by alphanumerics or underscores only",
      });
    });

    it("returns error for duplicate name within table", () => {
      const result = validateColumnName("email", ["id", "email", "name"]);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "name",
        message: "A column with this name already exists in this table",
      });
    });

    it("ignores excluded column ID when checking duplicates", () => {
      const result = validateColumnName("email", ["id", "email", "name"], "email");
      expect(result.valid).toBe(true);
    });
  });

  describe("validateDecimal", () => {
    it("returns valid for valid precision and scale", () => {
      const result = validateDecimal(10, 2);
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it("returns valid when precision equals scale", () => {
      const result = validateDecimal(5, 5);
      expect(result.valid).toBe(true);
    });

    it("returns error for precision < 1", () => {
      const result = validateDecimal(0, 2);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "precision",
        message: "Precision must be between 1 and 65",
      });
    });

    it("returns error for precision > 65", () => {
      const result = validateDecimal(66, 2);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "precision",
        message: "Precision must be between 1 and 65",
      });
    });

    it("returns error for missing precision", () => {
      const result = validateDecimal(undefined, 2);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "precision",
        message: "Precision must be between 1 and 65",
      });
    });

    it("returns error for scale < 0", () => {
      const result = validateDecimal(10, -1);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "scale",
        message: "Scale must be between 0 and precision",
      });
    });

    it("returns error for scale > precision", () => {
      const result = validateDecimal(10, 11);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "scale",
        message: "Scale must be between 0 and precision",
      });
    });

    it("returns error for missing scale", () => {
      const result = validateDecimal(10, undefined);
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "scale",
        message: "Scale must be between 0 and precision",
      });
    });
  });

  describe("validateEnumValues", () => {
    it("returns valid for valid enum values", () => {
      const result = validateEnumValues("active,inactive,pending");
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it("returns valid for single enum value", () => {
      const result = validateEnumValues("active");
      expect(result.valid).toBe(true);
    });

    it("returns error for empty input", () => {
      const result = validateEnumValues("");
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "enum_values",
        message: "Enum must have at least one value",
      });
    });

    it("returns error for whitespace-only input", () => {
      const result = validateEnumValues("   ");
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "enum_values",
        message: "Enum must have at least one value",
      });
    });

    it("returns error for invalid identifier in values", () => {
      const result = validateEnumValues("active,inactive,pending-value");
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "enum_values",
        message: "Each enum value must be a valid identifier",
      });
    });

    it("returns error for duplicate values", () => {
      const result = validateEnumValues("active,inactive,active");
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual({
        field: "enum_values",
        message: "Enum values must be unique",
      });
    });

    it("trims whitespace from values", () => {
      const result = validateEnumValues(" active , inactive , pending ");
      expect(result.valid).toBe(true);
    });
  });

  describe("Validation namespace", () => {
    it("exports all validation functions", () => {
      expect(Validation.isValidIdentifier).toBe(isValidIdentifier);
      expect(Validation.validateTableName).toBe(validateTableName);
      expect(Validation.validateColumnName).toBe(validateColumnName);
      expect(Validation.validateDecimal).toBe(validateDecimal);
      expect(Validation.validateEnumValues).toBe(validateEnumValues);
    });
  });
});