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
                    radius: 10,
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
    });
