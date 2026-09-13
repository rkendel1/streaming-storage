type WasmModule = {
  WasmArtifactSDK: any;
  WasmArtifactRecipe: any;
  WasmArtifactPipeline: any;
  memory?: WebAssembly.Memory;
};

let wasmModule: WasmModule | null = null;

export async function initializeWasm(wasmPath: string): Promise<void> {
  const buffer = await fetch(wasmPath).then(res => res.arrayBuffer());
  const { instance } = await WebAssembly.instantiate(buffer);
  wasmModule = instance.exports as WasmModule;
}

function ensureWasmInitialized(): WasmModule {
  if (!wasmModule) {
    throw new Error('WASM module not initialized. Call initializeWasm() first.');
  }
  return wasmModule;
}

export interface Capability {
  name: string;
  version: string;
}

export interface ArtifactEntry {
  path: string;
  entry_type: string;
}

export interface Artifact {
  identity: string;
  entries_count: number;
  entries: ArtifactEntry[];
}

export interface InspectedStage {
  label: string;
  identity: string;
}

export interface PipelineInspection {
  pipeline_identity: string;
  stages: InspectedStage[];
  required_capabilities: Capability[];
  materializer: string;
}

export interface AuthorizationDecision {
  allowed: boolean;
  requested_capabilities: Capability[];
  granted_capabilities: Capability[];
  denied_capabilities: Capability[];
}

export interface ExecutedStage {
  label: string;
  identity: string;
}

export interface ExecutionEvidence {
  is_successful: boolean;
  authorization_decision: AuthorizationDecision;
  stage_trace: ExecutedStage[];
  used_capabilities: Capability[];
}

export interface BuildResult {
  artifact: Artifact;
  evidence: ExecutionEvidence;
}

export type RecipeType = 'DirectoryZip' | 'DirectoryTar' | 'Wasm';

export interface RecipeConfig {
  [key: string]: any;
}

export interface RecipeSpecJSON {
  recipe_type: RecipeType;
  source: string;
  config: RecipeConfig;
}

export class ArtifactSDK {
  static recipeFromSpecJson(recipeJson: string): ArtifactRecipe {
    const wasm = ensureWasmInitialized();
    const recipe = new wasm.WasmArtifactRecipe();
    return new ArtifactRecipe(recipe, recipeJson);
  }

  static pipelineFromSpecJson(pipelineJson: string): ArtifactPipeline {
    const wasm = ensureWasmInitialized();
    const pipeline = new wasm.WasmArtifactPipeline();
    return new ArtifactPipeline(pipeline, pipelineJson);
  }

  static createDirectoryZipRecipe(source: string): ArtifactRecipe {
    const spec: RecipeSpecJSON = {
      recipe_type: 'DirectoryZip',
      source,
      config: {},
    };
    return ArtifactSDK.recipeFromSpecJson(JSON.stringify(spec));
  }

  static createDirectoryTarRecipe(source: string): ArtifactRecipe {
    const spec: RecipeSpecJSON = {
      recipe_type: 'DirectoryTar',
      source,
      config: {},
    };
    return ArtifactSDK.recipeFromSpecJson(JSON.stringify(spec));
  }

  static createWasmRecipe(source: string): ArtifactRecipe {
    const spec: RecipeSpecJSON = {
      recipe_type: 'Wasm',
      source,
      config: {},
    };
    return ArtifactSDK.recipeFromSpecJson(JSON.stringify(spec));
  }
}

export class ArtifactRecipe {
  private wasmRecipe: any;
  private specJson: string;

  constructor(wasmRecipe: any, specJson: string) {
    this.wasmRecipe = wasmRecipe;
    this.specJson = specJson;
  }

  compile(): ArtifactPipeline {
    try {
      const wasmPipeline = this.wasmRecipe.compile();
      return new ArtifactPipeline(wasmPipeline, this.wasmRecipe.spec_json());
    } catch (error) {
      throw new Error(`Recipe compilation failed: ${error}`);
    }
  }

  getSpecJson(): string {
    return this.specJson;
  }
}

export class ArtifactPipeline {
  private wasmPipeline: any;
  private specJson: string;

  constructor(wasmPipeline: any, specJson: string) {
    this.wasmPipeline = wasmPipeline;
    this.specJson = specJson;
  }

  inspect(): PipelineInspection {
    try {
      const inspectionJson = this.wasmPipeline.inspect();
      return JSON.parse(inspectionJson) as PipelineInspection;
    } catch (error) {
      throw new Error(`Pipeline inspection failed: ${error}`);
    }
  }

  requiredCapabilities(): Capability[] {
    try {
      const capsJson = this.wasmPipeline.required_capabilities();
      return JSON.parse(capsJson) as Capability[];
    } catch (error) {
      throw new Error(`Failed to get required capabilities: ${error}`);
    }
  }

  buildFromDirectory(root: string): Artifact {
    try {
      const artifactJson = this.wasmPipeline.build_from_directory(root);
      return JSON.parse(artifactJson) as Artifact;
    } catch (error) {
      throw new Error(`Build from directory failed: ${error}`);
    }
  }

  buildWithAuthorization(root: string): BuildResult {
    try {
      const resultJson = this.wasmPipeline.build_with_authorization(root);
      const result = JSON.parse(resultJson) as BuildResult;
      return result;
    } catch (error) {
      throw new Error(`Build with authorization failed: ${error}`);
    }
  }

  getSpecJson(): string {
    return this.specJson;
  }
}

export {
  Artifact,
  ArtifactEntry,
  Capability,
  InspectedStage,
  PipelineInspection,
  AuthorizationDecision,
  ExecutedStage,
  ExecutionEvidence,
  BuildResult,
  RecipeType,
  RecipeConfig,
  RecipeSpecJSON,
};
