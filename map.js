const map = L.map("map").setView([-37.81, 144.96], 13);

L.tileLayer("https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png", {
    attribution: "© OpenStreetMap contributors"
}).addTo(map);

fetch("points.geojson")
    .then(r => r.json())
    .then(data => {
        const layer = L.geoJSON(data, {
            pointToLayer: (feature, latlng) =>
                L.circleMarker(latlng, {
                    radius: 6,
                    fillOpacity: 0.8,
                    color: feature.properties.color,
                    label: feature.properties.label,
                }),

            onEachFeature: (feature, layer) => {
                layer.bindTooltip(feature.properties.label, {
                    permanent: false,
                    direction: "top",
                    offset: [0, -6]
                });
            }
        }).addTo(map);

        map.fitBounds(layer.getBounds(), {
            padding: [20, 20],
        });

        buildLegendFromData(data);
    });

function buildLegendFromData(data) {
    const mapLegend = new Map();

    const sortedFeatures = data.features
        .sort((a, b) => a.properties.label.localeCompare(b.properties.label))

    sortedFeatures.forEach(f => {
        mapLegend.set(
            f.properties.label,
            f.properties.color,
        );
    });

    const sortedLegend = Array.from(mapLegend.entries);
        // .sort((a, b) => a[0].localeCompare(b[0]))

    const legend = L.control({ position: "bottomright" });

    legend.onAdd = function () {
        const div = L.DomUtil.create("div", "legend");
        div.innerHTML = "<strong>Legend</strong><br>";

        mapLegend.forEach((color, name) => {
            div.innerHTML += `
                <div class="legend-item">
                    <span class="legend-color" style="background:${color}"></span>
                    ${name}
                </div>`;
        });

        return div;
    };

    legend.addTo(map);
}
