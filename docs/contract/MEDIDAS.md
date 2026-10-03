# Medidas: Marea en Quickshell contra Marea en pleamar

Las dos, en la misma máquina, en el mismo monitor y a la vez. Marea es la que
usa Abel todos los días (`~/Proyectos/proyecto-marea`, 370 ficheros QML);
`marea-plm` es esta carpeta.

Cómo se mide, para que se pueda repetir:

```sh
# CPU, memoria y hilos de un proceso, durante N segundos
medir.sh <pid> 30 "etiqueta"
```

Lee `/proc/<pid>/stat` dos veces separadas por N segundos para el tiempo de CPU,
y `/proc/<pid>/status` y `smaps_rollup` para la memoria. El PSS es lo que de
verdad ocupa: la memoria compartida, repartida entre quienes la comparten.

## 21 de septiembre de 2026 · el centro de control entero

Con el sonido y la sesión dentro, el panel a 520 × 510 y la superficie a
820 × 680. Medido con la barra abierta y animando, en HDMI-A-1 a 60 Hz.

| | CPU | RSS | PSS | frame |
| --- | --- | --- | --- | --- |
| marea-plm, animando | 8,6 % | 130,9 MB | 113,5 MB | 16,84 ms · p99 17,06 |

Y el susto del día, que es lo que de verdad hay que apuntar. Con la cadena de la
cara escrita `x + (k - x) * t` —cada eslabón nombrando al anterior **dos**
veces— y quince eslabones, la misma escena medía **41,09 ms de frame (24 fps),
100,6 % de CPU y 1 058 MB**. No era el runtime: la escena de ayer con el binario
de hoy iba a 16,8 ms. Era que un `let` se sustituye donde se nombra, así que el
árbol de la expresión **doblaba por eslabón**: 2¹⁵ nodos por propiedad,
evaluados dos veces por frame.

Se arregló en los dos sitios. En la escena, escribiendo los 48 eslabones como
`x * (1 - t) + k * t`, que nombra `x` una vez. Y en pleamar, que ya no sustituye
un `let` grande: lo calcula una vez por frame. Con eso, la escena que explotaba
vuelve a **16,78 ms y 8,87 %** sin tocarle una línea.

## 20 de septiembre de 2026 · la bolita

Solo la bolita: su cuerpo, su cara que respira, mira y parpadea, el aura y la
gota de los avisos. Marea, mientras tanto, es ella entera (tiene detrás sus
paneles, sus servicios y su cara con accesorios), así que **esta comparación no
es justa todavía**: es el punto de partida, no el resultado.

| | CPU | RSS | PSS | hilos | primer frame |
| --- | --- | --- | --- | --- | --- |
| Marea (Quickshell) | 12,98 % | 245,8 MB | 204,7 MB | 14 | sin medir |
| marea-plm, animando | 4,93 % | 111,7 MB | 94,1 MB | 13 | 175 ms |
| marea-plm, quieta | 0,32 % | 109,3 MB | 92,2 MB | 13 | 191 ms |

**Lo que dice.** Animando —respirando, parpadeando, mirando— pleamar cuesta la
tercera parte de CPU y menos de la mitad de memoria. Y **quieta baja a 0,3 %**,
porque el render duerme cuando no hay nada que mover: Quickshell sigue en 13 %
aunque no pase nada en la pantalla.

El 4,93 % de animar son unos 0,8 ms de CPU por frame a 60 Hz. De ahí sale la
primera deuda a mirar: la lista de dibujo se recompone entera cada frame, haya
cambiado algo o no (limitación P8 de pleamar).

## 20 de septiembre de 2026 · la tarjeta entera

Ya con todo: la bolita, la tarjeta que le sale del cuerpo, la hora, la fecha, el
volumen, los nueve escritorios con su píldora y el título de la ventana de
delante. Son 62 instrucciones, 14 propiedades animadas y 11 zonas.

| | CPU | RSS | PSS | hilos |
| --- | --- | --- | --- | --- |
| Marea (Quickshell) | 13,15 % | 197,4 MB | 156,5 MB | 14 |
| marea-plm, tarjeta abierta | 5,32 % | 112,9 MB | 96,0 MB | 17 |
| marea-plm, tarjeta cerrada | 5,05 % | 112,9 MB | 96,0 MB | 17 |

**Lo que dice.** Abrir la tarjeta entera cuesta un 0,3 % más que tenerla cerrada.
Dicho de otro modo: **lo que se dibuja casi no importa**. Y eso vale también al
revés, que es lo interesante: pasar de un círculo a una barra completa no movió
la aguja.

## De dónde sale ese 5 %

Con `PLEAMAR_CRONO=1`, pleamar imprime el desglose de cada frame:

```
crono · hueco 16,04 · apuntar 0,10 · cerrar 0,21 · mandar+presentar 0,23 ms
```

Los 16 ms de «pedir el hueco» son la espera del monitor a 60 Hz, y son **sueño**,
no CPU. Lo que se gasta de verdad son los 0,5 ms de apuntar las órdenes, cerrar
el mando y mandarlo: el suelo de wgpu sobre Vulkan, que hay que pagar aunque solo
se mueva un círculo. Queda anotado en pleamar como la limitación P11, con su plan.

Para comparar: Quickshell gasta unos 2 ms de CPU por frame haciendo lo mismo.

## 20 de septiembre de 2026 · con los efectos puestos

Encima de lo anterior: la píldora del escritorio **se estira mientras viaja**
(con la velocidad de su propio muelle, que solo el render conoce), cada escritorio
**se realza** al pasar por encima con su muelle propio, y al cambiar el minuto **la
hora entra desde arriba** con un empujón que devuelve el muelle. Son 24 propiedades
animadas, 71 instrucciones y 32 reglas.

| | CPU | RSS | PSS | hilos |
| --- | --- | --- | --- | --- |
| Marea (Quickshell) | 13,16 % | 188,1 MB | 147,3 MB | 14 |
| marea-plm, con los efectos | 5,40 % | 113,1 MB | 96,4 MB | 17 |

**Lo que dice.** Diez propiedades animadas más, tres efectos nuevos y diecinueve
reglas más: **0,08 % de CPU**. Añadir movimiento aquí no cuesta, porque el gasto
es el suelo de presentar el frame, y ese ya se paga.

Eso es lo contrario de lo que pasa en QtQuick, donde cada `NumberAnimation` y
cada `Behavior` es un objeto que se evalúa en el hilo de la interfaz.

## Un fallo que destapó escribir esto

La tarjeta prometía llegar sin una línea de lógica, y no llegaba: si una escena
no tenía su `.luau` al lado, pleamar le daba un guion tonto que no montaba sus
servicios, y el reloj se quedaba en `--:--`. Arreglado en pleamar el mismo día
(commit «Una escena sin lógica también monta sus servicios»). Escribir de verdad
encuentra lo que ninguna lista encuentra.

## 20 de septiembre de 2026 · frames y arranque

Con permiso de Abel, Marea se relanzó con el cronómetro de Qt
(`QSG_RENDER_TIMING=1`) y se devolvió a su sitio, con su entorno exacto, en menos
de un minuto.

**Cuánto tarda en aparecer:**

| | hasta el primer frame |
| --- | --- |
| Marea (Quickshell) | 1876 ms (1579 solo en leer su configuración) |
| marea-plm | 175 ms |

**Qué hace por frame**, mientras se la deja quieta:

| | frames en 20 s | coste de cada uno |
| --- | --- | --- |
| Marea (Quickshell) | 475 (~24 por segundo) | 0,61 ms · polish 0,05 · sync 0,12 · render 0,11 · swap 0,16 |
| marea-plm | 1200 (60 por segundo) | 0,50 ms · apuntar 0,10 · cerrar 0,21 · mandar 0,23 |

**Lo que dice, y es lo más interesante de todo.** Marea pinta **la mitad de
frames** que marea-plm y cada frame le cuesta **lo mismo**, pero gasta **dos veces
y media más CPU**. O sea: su gasto no está en pintar. Está en lo de alrededor —los
enlaces de QML, el JavaScript, sus servicios—, que es justo lo que en pleamar no
existe porque lo que se declara lo ejecuta el render y la lógica solo cuenta cosas.

Y cuando no hay nada que mover, marea-plm baja a 0,3 % y Marea no: sigue en su
sitio.

## 20 de septiembre de 2026 · la medida justa

Las dos a la vez, las dos recién arrancadas, las dos animándose en el mismo
monitor. Esta es la que vale.

| | CPU | memoria real (PSS) | frames por segundo | CPU por frame pintado |
| --- | --- | --- | --- | --- |
| Marea (Quickshell) | 6,17 % | 335,8 MB | ~24 | 0,257 % |
| marea-plm | 4,96 % | 81,3 MB | 60 | 0,083 % |

**Lo que dice.** En CPU a secas la diferencia es de un 20 %, no de tres veces: hay
que decirlo. Pero marea-plm está pintando **dos veces y media más frames**. Por
cada frame que sale a la pantalla, pleamar gasta **tres veces menos**.

Y en memoria no hay discusión: **cuatro veces menos**, con la misma barra.

## 20 de septiembre de 2026 · abrir y cerrar

Abrir la tarjeta de cada una, medir, cerrarla y volver a medir. A Marea se le pide
por su propia puerta (`quickshell ipc call marea open`), sin tocarle el ratón a
nadie; a marea-plm, con `pleamar --decir`.

| | cerrada | abierta | cerrada otra vez |
| --- | --- | --- | --- |
| Marea (Quickshell) | 6,17 % | 12,28 % | 10,40 %, y a los 30 s todavía 9,50 % |
| marea-plm | 4,39 % | 4,89 % | 4,50 % |

**Lo que dice.** Abrir la tarjeta de Marea **dobla** su CPU, y al cerrarla **no
vuelve**: se queda a mitad de camino y ahí sigue medio minuto después. Lo que se
instanció al abrirla se queda instanciado, con sus enlaces vivos.

En marea-plm abrir cuesta medio punto y cerrar lo devuelve entero, porque no se
instancia nada: la tarjeta ya estaba declarada, y lo que cambia es un número que
el render anima.

**Aviso de honradez**: la tarjeta de Marea lleva dentro mucho más de lo que lleva
la de marea-plm (paneles, chat, agentes). Los 12,28 % no son «lo mismo dibujado
más caro». Lo que sí es comparable es **la forma**: en QML abrir algo crea objetos
que se quedan; aquí abrir es mover una propiedad.

## Una advertencia sobre estas medidas

Marea recién arrancada gasta 5,75 % de CPU y 386 MB; la misma Marea llevando
horas corriendo, 13,2 % y 188 MB. Los números de Quickshell **se mueven** con el
rato que lleve encendida y con lo que haya abierto. Los de pleamar, medidos tres
veces en tres estados distintos, se quedaron entre 5,0 y 5,4 %.

Cuando se vuelva a medir, medir las dos a la vez y con el mismo rato encendidas.

## Lo que falta por medir

- **GPU**: `nvidia-smi` da 926 MiB y 7 % para toda la sesión, no por proceso.
- La tarjeta de Marea abierta con sus paneles dentro, que es cuando Quickshell
  instancia de verdad. Pide tocarle la barra a Abel mientras se mide.
- Y lo mismo en un rato largo: una hora encendidas las dos.
