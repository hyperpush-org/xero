import { z } from 'zod'

export const developerStorageSourceKindSchema = z.enum(['global_sqlite', 'project_lance'])
export type DeveloperStorageSourceKindDto = z.infer<typeof developerStorageSourceKindSchema>

export const developerStorageSourceSchema = z
  .object({
    kind: developerStorageSourceKindSchema,
    projectId: z.string().trim().min(1).nullable().optional(),
  })
  .strict()
export type DeveloperStorageSourceDto = z.infer<typeof developerStorageSourceSchema>

export const developerStorageColumnSchema = z
  .object({
    name: z.string().trim().min(1),
    typeLabel: z.string(),
  })
  .strict()
export type DeveloperStorageColumnDto = z.infer<typeof developerStorageColumnSchema>

export const developerStorageTableSummarySchema = z
  .object({
    name: z.string().trim().min(1),
    columns: z.array(developerStorageColumnSchema),
    rowCount: z.number().int().nonnegative(),
  })
  .strict()
export type DeveloperStorageTableSummaryDto = z.infer<typeof developerStorageTableSummarySchema>

export const developerSqliteDatabaseSchema = z
  .object({
    path: z.string(),
    tables: z.array(developerStorageTableSummarySchema),
  })
  .strict()
export type DeveloperSqliteDatabaseDto = z.infer<typeof developerSqliteDatabaseSchema>

export const developerProjectLanceDatabaseSchema = z
  .object({
    projectId: z.string().trim().min(1),
    projectName: z.string().trim().min(1),
    repositoryRoot: z.string(),
    stateDatabasePath: z.string(),
    lancePath: z.string(),
    exists: z.boolean(),
    tables: z.array(developerStorageTableSummarySchema),
  })
  .strict()
export type DeveloperProjectLanceDatabaseDto = z.infer<typeof developerProjectLanceDatabaseSchema>

export const developerStorageOverviewSchema = z
  .object({
    globalSqlite: developerSqliteDatabaseSchema,
    projectLance: z.array(developerProjectLanceDatabaseSchema),
  })
  .strict()
export type DeveloperStorageOverviewDto = z.infer<typeof developerStorageOverviewSchema>

export const developerReadStorageTableRequestSchema = z
  .object({
    source: developerStorageSourceSchema,
    tableName: z.string().trim().min(1),
    limit: z.number().int().positive().max(200).optional(),
    offset: z.number().int().nonnegative().optional(),
    revealSensitive: z.boolean().optional(),
  })
  .strict()
export type DeveloperReadStorageTableRequestDto = z.infer<typeof developerReadStorageTableRequestSchema>

export const developerStorageRowSchema = z
  .object({
    values: z.record(z.string(), z.unknown()),
    displayValues: z.record(z.string(), z.string()),
  })
  .strict()
export type DeveloperStorageRowDto = z.infer<typeof developerStorageRowSchema>

export const developerStorageTableRowsSchema = z
  .object({
    source: developerStorageSourceSchema,
    tableName: z.string().trim().min(1),
    path: z.string(),
    columns: z.array(developerStorageColumnSchema),
    rows: z.array(developerStorageRowSchema),
    rowCount: z.number().int().nonnegative(),
    limit: z.number().int().positive(),
    offset: z.number().int().nonnegative(),
    redacted: z.boolean(),
  })
  .strict()
export type DeveloperStorageTableRowsDto = z.infer<typeof developerStorageTableRowsSchema>
