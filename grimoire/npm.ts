// A search-driven list: the spell reads the query, calls an API with Effect's
// HttpClient, and decodes the response with Schema.
import * as Effect from "effect/Effect"
import * as HttpClient from "effect/http/HttpClient"
import * as HttpClientResponse from "effect/http/HttpClientResponse"
import * as Schema from "effect/Schema"
import { Action, Spell, Ui } from "cantrip"

const SearchResult = Schema.Struct({
  objects: Schema.Array(Schema.Struct({
    package: Schema.Struct({
      name: Schema.String,
      version: Schema.String,
      description: Schema.optional(Schema.String),
      links: Schema.Struct({ npm: Schema.String, repository: Schema.optional(Schema.String) })
    })
  }))
})

const search = Effect.fn("npm.search")(function*(query: string) {
  if (query.trim() === "") return []
  const client = (yield* HttpClient.HttpClient).pipe(HttpClient.filterStatusOk)
  const response = yield* client.get(
    `https://registry.npmjs.org/-/v1/search?size=25&text=${encodeURIComponent(query)}`
  )
  const result = yield* HttpClientResponse.schemaBodyJson(SearchResult)(response)
  return result.objects.map(({ package: pkg }) => ({
    id: pkg.name,
    title: pkg.name,
    subtitle: pkg.description,
    icon: "shippingbox",
    accessories: [pkg.version],
    actions: [
      Action.open("Open on npm", pkg.links.npm),
      Action.copy("Copy Install Command", `bun add ${pkg.name}`),
      ...(pkg.links.repository ? [Action.open("Open Repository", pkg.links.repository)] : [])
    ]
  }))
})

export default Spell.make({
  title: "Search npm",
  subtitle: "Example: a list that searches as you type",
  icon: "shippingbox",
  keywords: ["package", "registry", "bun"],
  run: Ui.searchList({
    placeholder: "Search packages…",
    emptyText: "Type to search npm",
    search
  })
})
