use crate::dtos::db::codex::ReadShape;

/// The root relation of an aggregate. Every aggregate has exactly one.
#[derive(Debug, Clone)]
pub struct CodexRoot {
    pub relation: String,
    pub id_column: String,
}

#[derive(Debug, Clone)]
pub struct CodexEntity {
    pub name: String,
    pub display_name: String,
    pub relation: String,
    pub reference_column: String,
}

/// Current-state sink relation. `entity == None` means the root sink.
#[derive(Debug, Clone)]
pub struct CodexSink {
    pub relation: String,
    pub entity: Option<String>,
}

/// A declared read surface — a view Mason can query. .
#[derive(Debug, Clone)]
pub struct CodexSurface {
    pub view: String,
    pub entity: Option<String>,
    pub shape: ReadShape,
}

/// Everything Codex knows about one aggregate.
#[derive(Debug, Clone)]
pub struct CodexAggregate {
    pub name: String,
    pub root: CodexRoot,
    pub entities: Vec<CodexEntity>,
    pub sinks: Vec<CodexSink>,
    pub surfaces: Vec<CodexSurface>,
}

impl CodexAggregate {
    pub fn surface(&self, entity: Option<&str>, shape: &ReadShape) -> Option<&CodexSurface> {
        self.surfaces
            .iter()
            .find(|surface| surface.entity.as_deref() == entity && &surface.shape == shape)
    }

    pub fn list_surface(&self, surface_name: &str) -> Option<&CodexSurface> {
        self.surface(None, &ReadShape::List(surface_name.to_string()))
    }

    pub fn one_surface(&self, surface_name: &str) -> Option<&CodexSurface> {
        self.surface(None, &ReadShape::Detail(surface_name.to_string()))
            .or_else(|| self.surface(None, &ReadShape::List(surface_name.to_string())))
    }

    pub fn entity_list_surface(&self, entity_name: &str, surface_name: &str) -> Option<&CodexSurface> {
        self.surface(Some(entity_name), &ReadShape::List(surface_name.to_string()))
    }

    pub fn entity(&self, name: &str) -> Option<&CodexEntity> {
        self.entities.iter().find(|entity| entity.name == name)
    }

    pub fn sink(&self, entity: Option<&str>) -> Option<&CodexSink> {
        self.sinks.iter().find(|sink| sink.entity.as_deref() == entity)
    }
}
